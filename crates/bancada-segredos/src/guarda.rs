//! [`GuardaDeSegredos`]: a interface única para o app.

use crate::arquivo_cifrado::{self, ArquivoCifrado, Chave, Derivacao, ParametrosDeDerivacao};
use crate::chaveiro::{ChaveiroDoSistema, ChaveiroNativo};
use crate::conteudo::{self, Mapa};
use crate::erro::{ErroDeSegredo, TAMANHO_MINIMO_DA_SENHA};
use crate::gravacao::{criar_diretorio_privado, gravar_atomicamente};
use crate::indice::Indice;
use crate::rotulo;
use crate::segredo::Segredo;
use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// Nome padrão do arquivo cifrado dentro do diretório da guarda.
pub const ARQUIVO_CIFRADO_PADRAO: &str = "segredos.cifrado";
/// Nome do índice de rótulos dentro do diretório da guarda.
pub const ARQUIVO_DO_INDICE: &str = "rotulos.json";

/// Onde a guarda guarda os segredos agora.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Situacao {
    /// Keyring do sistema operacional.
    ChaveiroDoSistema,
    /// Arquivo cifrado, trancado: a senha-mestra é pedida quando um segredo for necessário.
    ArquivoTrancado,
    /// Arquivo cifrado, destrancado.
    ArquivoAberto,
    /// Sem keyring e sem arquivo cifrado: nada pode ser guardado até o usuário criar o
    /// arquivo com uma senha-mestra (nunca há gravação em texto puro).
    Indisponivel,
}

/// Configuração da guarda. O padrão serve ao app; os testes trocam o keyring (por um falso) e o
/// custo do Argon2id.
#[derive(Debug, Clone)]
pub struct ConfiguracaoDaGuarda {
    servico: String,
    diretorio: PathBuf,
    arquivo_cifrado: String,
    chaveiro: Arc<dyn ChaveiroDoSistema>,
    derivacao: ParametrosDeDerivacao,
}

impl ConfiguracaoDaGuarda {
    /// Guarda do app `servico` (o nome sob o qual os segredos ficam no keyring, por exemplo
    /// "Seu Bolso Feliz"), com índice e arquivo cifrado em `diretorio`, e o keyring de verdade.
    pub fn nova(servico: impl Into<String>, diretorio: impl AsRef<Path>) -> Self {
        Self {
            servico: servico.into(),
            diretorio: diretorio.as_ref().to_path_buf(),
            arquivo_cifrado: ARQUIVO_CIFRADO_PADRAO.to_owned(),
            chaveiro: Arc::new(ChaveiroNativo),
            derivacao: ParametrosDeDerivacao::default(),
        }
    }

    /// Outro keyring (o falso, nos testes).
    pub fn chaveiro(mut self, chaveiro: Arc<dyn ChaveiroDoSistema>) -> Self {
        self.chaveiro = chaveiro;
        self
    }

    /// Custo do Argon2id ao criar o arquivo ou trocar a senha (ao abrir, vale o do arquivo).
    pub fn derivacao(mut self, parametros: ParametrosDeDerivacao) -> Self {
        self.derivacao = parametros;
        self
    }

    /// Nome do arquivo cifrado dentro do diretório (o Chess Studio usa `own.vault`).
    pub fn arquivo_cifrado(mut self, nome: impl Into<String>) -> Self {
        self.arquivo_cifrado = nome.into();
        self
    }
}

enum Base {
    Chaveiro,
    ArquivoTrancado,
    ArquivoAberto {
        derivacao: Derivacao,
        chave: Chave,
        segredos: Mapa,
    },
    Indisponivel,
}

impl Base {
    fn situacao(&self) -> Situacao {
        match self {
            Self::Chaveiro => Situacao::ChaveiroDoSistema,
            Self::ArquivoTrancado => Situacao::ArquivoTrancado,
            Self::ArquivoAberto { .. } => Situacao::ArquivoAberto,
            Self::Indisponivel => Situacao::Indisponivel,
        }
    }
}

struct Estado {
    base: Base,
    indice: Indice,
}

struct Interno {
    servico: String,
    arquivo: PathBuf,
    indice: PathBuf,
    chaveiro: Arc<dyn ChaveiroDoSistema>,
    derivacao: ParametrosDeDerivacao,
    estado: Mutex<Estado>,
}

/// A guarda de segredos. Barata de clonar; os clones compartilham o estado e podem ir para
/// outras threads (o Argon2id é lento de propósito: crie, destranque e troque a senha fora da
/// thread da interface).
///
/// - **Keyring do sistema primeiro.** Sem ele, o usuário cria o arquivo cifrado
///   ([`GuardaDeSegredos::criar_arquivo`]); até lá nada é guardado. Nunca há gravação em texto
///   puro nem troca de lugar em silêncio: havendo o arquivo cifrado, é ele que vale.
/// - **Arquivo cifrado** destrancado sob demanda, trancável, com troca de senha-mestra.
/// - O `Debug` não mostra segredo nem rótulo; valores, chave e conteúdo decifrado são zerados
///   na memória ao descartar.
#[derive(Clone)]
pub struct GuardaDeSegredos {
    interno: Arc<Interno>,
}

impl fmt::Debug for GuardaDeSegredos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GuardaDeSegredos")
            .field("servico", &self.interno.servico)
            .field("situacao", &self.situacao())
            .field("rotulos", &self.rotulos().len())
            .finish_non_exhaustive()
    }
}

fn travar<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn validar_senha(senha: &Segredo) -> Result<(), ErroDeSegredo> {
    if senha.expor().chars().count() < TAMANHO_MINIMO_DA_SENHA {
        return Err(ErroDeSegredo::SenhaFraca {
            minimo: TAMANHO_MINIMO_DA_SENHA,
        });
    }
    Ok(())
}

fn existe(caminho: &Path) -> Result<bool, ErroDeSegredo> {
    caminho.try_exists().map_err(|e| ErroDeSegredo::disco(&e))
}

fn gravar_arquivo(
    caminho: &Path,
    derivacao: &Derivacao,
    chave: &Chave,
    segredos: &Mapa,
) -> Result<(), ErroDeSegredo> {
    let conteudo = conteudo::codificar(segredos)?;
    let arquivo = arquivo_cifrado::selar(derivacao, chave, &conteudo)?;
    gravar_atomicamente(caminho, &arquivo)
}

fn ler_arquivo(caminho: &Path, senha: &Segredo) -> Result<(Derivacao, Chave, Mapa), ErroDeSegredo> {
    let bytes = fs::read(caminho).map_err(|e| ErroDeSegredo::disco(&e))?;
    let aberto = ArquivoCifrado::ler(&bytes)?.destrancar(senha.expor().as_bytes())?;
    let segredos = conteudo::decodificar(aberto.versao, &aberto.conteudo)?;
    Ok((aberto.derivacao, aberto.chave, segredos))
}

impl Estado {
    /// Põe no índice exatamente `rotulos` e grava, se mudou. Só altera a memória depois de
    /// gravar.
    fn indice_igual_a(
        &mut self,
        caminho: &Path,
        rotulos: BTreeSet<String>,
    ) -> Result<(), ErroDeSegredo> {
        if self.indice.rotulos == rotulos {
            return Ok(());
        }
        let novo = Indice { rotulos };
        novo.salvar(caminho)?;
        self.indice = novo;
        Ok(())
    }
}

impl GuardaDeSegredos {
    /// Abre a guarda: o arquivo cifrado se ele existe (trancado), senão o keyring do sistema
    /// se ele responde, senão [`Situacao::Indisponivel`]. Cria o diretório (0700 no Unix), mas
    /// não grava arquivo nenhum.
    pub fn abrir(configuracao: ConfiguracaoDaGuarda) -> Result<Self, ErroDeSegredo> {
        rotulo::validar(&configuracao.servico)?;
        criar_diretorio_privado(&configuracao.diretorio)?;
        let arquivo = configuracao.diretorio.join(&configuracao.arquivo_cifrado);
        let caminho_do_indice = configuracao.diretorio.join(ARQUIVO_DO_INDICE);
        let indice = Indice::carregar(&caminho_do_indice)?;
        let base = if existe(&arquivo)? {
            Base::ArquivoTrancado
        } else if configuracao.chaveiro.esta_disponivel() {
            Base::Chaveiro
        } else {
            Base::Indisponivel
        };
        Ok(Self {
            interno: Arc::new(Interno {
                servico: configuracao.servico,
                arquivo,
                indice: caminho_do_indice,
                chaveiro: configuracao.chaveiro,
                derivacao: configuracao.derivacao,
                estado: Mutex::new(Estado { base, indice }),
            }),
        })
    }

    pub fn situacao(&self) -> Situacao {
        travar(&self.interno.estado).base.situacao()
    }

    /// Os rótulos guardados, em ordem, sem os valores. Com o arquivo trancado (ou com o
    /// keyring), vêm do índice; com o arquivo aberto, do conteúdo decifrado.
    pub fn rotulos(&self) -> Vec<String> {
        let estado = travar(&self.interno.estado);
        match &estado.base {
            Base::ArquivoAberto { segredos, .. } => segredos.keys().cloned().collect(),
            _ => estado.indice.rotulos.iter().cloned().collect(),
        }
    }

    // ---------------- Arquivo cifrado ----------------

    /// Cria o arquivo cifrado com a senha-mestra (só quando o sistema não oferece keyring e
    /// ainda não há arquivo). Fica destrancado.
    pub fn criar_arquivo(&self, senha: &Segredo) -> Result<(), ErroDeSegredo> {
        validar_senha(senha)?;
        if self.situacao() != Situacao::Indisponivel {
            return Err(ErroDeSegredo::NaoPodeCriarArquivo);
        }
        // O Argon2id é lento de propósito: fora da trava, para `situacao()` seguir instantânea.
        let (derivacao, chave) =
            arquivo_cifrado::derivar_nova(self.interno.derivacao, senha.expor().as_bytes())?;
        let mut estado = travar(&self.interno.estado);
        if !matches!(estado.base, Base::Indisponivel) || existe(&self.interno.arquivo)? {
            return Err(ErroDeSegredo::NaoPodeCriarArquivo);
        }
        let segredos = Mapa::new();
        gravar_arquivo(&self.interno.arquivo, &derivacao, &chave, &segredos)?;
        estado.base = Base::ArquivoAberto {
            derivacao,
            chave,
            segredos,
        };
        estado.indice_igual_a(&self.interno.indice, BTreeSet::new())
    }

    /// Destranca o arquivo cifrado. Senha errada e arquivo adulterado, cortado ou corrompido
    /// dão o mesmo [`ErroDeSegredo::SenhaOuArquivoInvalido`]. O índice é regravado a partir
    /// do conteúdo, se divergir.
    pub fn destrancar(&self, senha: &Segredo) -> Result<(), ErroDeSegredo> {
        if matches!(
            self.situacao(),
            Situacao::ChaveiroDoSistema | Situacao::Indisponivel
        ) {
            return Err(ErroDeSegredo::SemArquivoCifrado);
        }
        let (derivacao, chave, segredos) = ler_arquivo(&self.interno.arquivo, senha)?;
        let mut estado = travar(&self.interno.estado);
        match estado.base {
            Base::Chaveiro | Base::Indisponivel => return Err(ErroDeSegredo::SemArquivoCifrado),
            // Outro clone destrancou (e talvez gravou) enquanto o Argon2id rodava: o que está
            // em memória é o mais novo.
            Base::ArquivoAberto { .. } => return Ok(()),
            Base::ArquivoTrancado => {}
        }
        let rotulos = segredos.keys().cloned().collect();
        estado.base = Base::ArquivoAberto {
            derivacao,
            chave,
            segredos,
        };
        estado.indice_igual_a(&self.interno.indice, rotulos)
    }

    /// Tranca o arquivo cifrado: chave e valores saem da memória (zerados).
    pub fn trancar(&self) {
        let mut estado = travar(&self.interno.estado);
        if matches!(estado.base, Base::ArquivoAberto { .. }) {
            estado.base = Base::ArquivoTrancado;
        }
    }

    /// Troca a senha-mestra: confere a atual contra o arquivo, gera sal e chave novos e regrava
    /// (sempre no formato 2). Funciona trancado ou aberto; fica aberto.
    pub fn trocar_senha_mestra(
        &self,
        atual: &Segredo,
        nova: &Segredo,
    ) -> Result<(), ErroDeSegredo> {
        validar_senha(nova)?;
        if matches!(
            self.situacao(),
            Situacao::ChaveiroDoSistema | Situacao::Indisponivel
        ) {
            return Err(ErroDeSegredo::SemArquivoCifrado);
        }
        let (_, _, do_arquivo) = ler_arquivo(&self.interno.arquivo, atual)?;
        let (derivacao, chave) =
            arquivo_cifrado::derivar_nova(self.interno.derivacao, nova.expor().as_bytes())?;
        let mut estado = travar(&self.interno.estado);
        let segredos = match &estado.base {
            Base::Chaveiro | Base::Indisponivel => return Err(ErroDeSegredo::SemArquivoCifrado),
            // Aberto: vale o que está em memória (igual ao arquivo ou mais novo).
            Base::ArquivoAberto { segredos, .. } => segredos.clone(),
            Base::ArquivoTrancado => do_arquivo,
        };
        gravar_arquivo(&self.interno.arquivo, &derivacao, &chave, &segredos)?;
        let rotulos = segredos.keys().cloned().collect();
        estado.base = Base::ArquivoAberto {
            derivacao,
            chave,
            segredos,
        };
        estado.indice_igual_a(&self.interno.indice, rotulos)
    }

    // ---------------- Segredos ----------------

    /// Grava (ou substitui) o segredo do rótulo. O anterior é descartado e zerado.
    pub fn gravar(&self, rotulo: &str, valor: Segredo) -> Result<(), ErroDeSegredo> {
        rotulo::validar(rotulo)?;
        if valor.esta_vazio() {
            return Err(ErroDeSegredo::SegredoVazio);
        }
        if self.exigir_aberta()? == Situacao::ChaveiroDoSistema {
            // O keyring pode demorar (D-Bus, pedido de desbloqueio): fora da trava.
            self.interno
                .chaveiro
                .gravar(&self.interno.servico, rotulo, &valor)?;
        }
        let mut estado = travar(&self.interno.estado);
        let Estado { base, indice } = &mut *estado;
        match base {
            Base::Indisponivel => return Err(ErroDeSegredo::Indisponivel),
            Base::ArquivoTrancado => return Err(ErroDeSegredo::Trancado),
            Base::Chaveiro => {}
            Base::ArquivoAberto {
                derivacao,
                chave,
                segredos,
            } => {
                let mut novos = segredos.clone();
                novos.insert(rotulo.to_owned(), valor);
                gravar_arquivo(&self.interno.arquivo, derivacao, chave, &novos)?;
                *segredos = novos;
            }
        }
        let mut rotulos = indice.rotulos.clone();
        rotulos.insert(rotulo.to_owned());
        estado.indice_igual_a(&self.interno.indice, rotulos)
    }

    /// Lê o segredo do rótulo. Com o keyring, vai direto a ele (mesmo que o índice não conheça
    /// o rótulo).
    pub fn ler(&self, rotulo: &str) -> Result<Segredo, ErroDeSegredo> {
        rotulo::validar(rotulo)?;
        {
            let estado = travar(&self.interno.estado);
            match &estado.base {
                Base::Indisponivel => return Err(ErroDeSegredo::Indisponivel),
                Base::ArquivoTrancado => return Err(ErroDeSegredo::Trancado),
                Base::ArquivoAberto { segredos, .. } => {
                    return segredos
                        .get(rotulo)
                        .cloned()
                        .ok_or(ErroDeSegredo::NaoEncontrado);
                }
                Base::Chaveiro => {}
            }
        }
        // O keyring pode demorar: fora da trava.
        self.interno
            .chaveiro
            .ler(&self.interno.servico, rotulo)?
            .ok_or(ErroDeSegredo::NaoEncontrado)
    }

    /// Apaga o segredo do rótulo. Apagar o que não existe não é erro. Com o arquivo trancado é
    /// [`ErroDeSegredo::Trancado`]: nada é marcado para apagar depois.
    pub fn apagar(&self, rotulo: &str) -> Result<(), ErroDeSegredo> {
        rotulo::validar(rotulo)?;
        if self.exigir_aberta()? == Situacao::ChaveiroDoSistema {
            // Fora da trava; se o keyring falhar, o rótulo fica no índice (o segredo continua
            // lá e a guarda não finge que apagou).
            self.interno
                .chaveiro
                .apagar(&self.interno.servico, rotulo)?;
        }
        let mut estado = travar(&self.interno.estado);
        let Estado { base, indice } = &mut *estado;
        match base {
            Base::Indisponivel => return Err(ErroDeSegredo::Indisponivel),
            Base::ArquivoTrancado => return Err(ErroDeSegredo::Trancado),
            Base::Chaveiro => {}
            Base::ArquivoAberto {
                derivacao,
                chave,
                segredos,
            } => {
                if segredos.contains_key(rotulo) {
                    let mut novos = segredos.clone();
                    novos.remove(rotulo);
                    gravar_arquivo(&self.interno.arquivo, derivacao, chave, &novos)?;
                    *segredos = novos;
                }
            }
        }
        let mut rotulos = indice.rotulos.clone();
        rotulos.remove(rotulo);
        estado.indice_igual_a(&self.interno.indice, rotulos)
    }

    /// A situação, se ela permite gravar ou apagar agora.
    fn exigir_aberta(&self) -> Result<Situacao, ErroDeSegredo> {
        match self.situacao() {
            Situacao::Indisponivel => Err(ErroDeSegredo::Indisponivel),
            Situacao::ArquivoTrancado => Err(ErroDeSegredo::Trancado),
            situacao => Ok(situacao),
        }
    }
}
