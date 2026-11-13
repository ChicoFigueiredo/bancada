//! [`Rede`]: o ponto único por onde todo pedido passa (o portão), as filas por host, o cache
//! com `ETag` e o estado da rede.
//!
//! Modelo de threads (como o `chess-net` de onde veio): cada host do catálogo tem uma thread
//! própria com fila, criada no primeiro pedido por [`Rede::buscar`]; os pedidos de um host saem
//! um de cada vez. O resultado volta por canal e a interface consulta
//! [`Pendente::tentar_pegar`] a cada quadro, sem bloquear. Quem já está numa thread de trabalho
//! (a sincronização) usa [`Rede::executar`], que roda o mesmo portão na thread de quem chama.

use crate::autorizacao::{Autorizacao, FonteDeAutorizacao, SemAutorizacao};
use crate::cache::{Cache, Guardada};
use crate::catalogo::{Catalogo, Endereco};
use crate::erro::{CorpoDeErro, ErroDeRede};
use crate::estado::{EstadoDaRede, Operacao, SituacaoDoHost};
use crate::limites::{
    Balde, PAUSA_PADRAO, PoliticaDeRepeticao, Ritmo, SplitMix64, tentar_de_novo_em,
};
use crate::pedido::{Alvo, Metodo, Pedido, sanitizado};
use crate::relogio::{Instante, Relogio, RelogioDoSistema};
use crate::resposta::{Frescor, Resposta};
use crate::transporte::{Cabeca, Falha, Http, Saida};
use std::collections::hash_map::RandomState;
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::hash::{BuildHasher, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Duration;

const PRAZO_PADRAO: Duration = Duration::from_secs(30);
const PRAZO_DE_CONEXAO_PADRAO: Duration = Duration::from_secs(10);
const LIMITE_DE_CORPO_PADRAO: usize = 64 * 1024 * 1024;
/// Quantas pausas (`429`) seguidas um pedido paciente aceita antes de desistir.
const MAXIMO_DE_PAUSAS: u32 = 3;
const OPERACOES_GUARDADAS: usize = 100;

#[derive(Debug, Clone)]
enum LugarDoCache {
    Nenhum,
    Memoria,
    Arquivo(PathBuf),
}

/// Configuração do portão. Os padrões servem ao app; os testes trocam o relógio, as rotas
/// (servidor falso) e os limites.
#[derive(Debug, Clone)]
pub struct ConfiguracaoDaRede {
    catalogo: Catalogo,
    cache: LugarDoCache,
    relogio: Arc<dyn Relogio>,
    rotas: Vec<(String, String)>,
    ritmos: HashMap<String, Ritmo>,
    ritmo_padrao: Ritmo,
    repeticao: PoliticaDeRepeticao,
    autorizacao: Arc<dyn FonteDeAutorizacao>,
    prazo: Duration,
    prazo_de_conexao: Duration,
    limite_de_corpo: usize,
    semente: Option<u64>,
    agente_de_usuario: String,
}

impl ConfiguracaoDaRede {
    /// Portão com o `catalogo`, sem cache e sem credencial.
    pub fn nova(catalogo: Catalogo) -> Self {
        Self {
            catalogo,
            cache: LugarDoCache::Nenhum,
            relogio: Arc::new(RelogioDoSistema),
            rotas: Vec::new(),
            ritmos: HashMap::new(),
            ritmo_padrao: Ritmo::default(),
            repeticao: PoliticaDeRepeticao::default(),
            autorizacao: Arc::new(SemAutorizacao),
            prazo: PRAZO_PADRAO,
            prazo_de_conexao: PRAZO_DE_CONEXAO_PADRAO,
            limite_de_corpo: LIMITE_DE_CORPO_PADRAO,
            semente: None,
            agente_de_usuario: concat!("bancada-rede/", env!("CARGO_PKG_VERSION")).to_owned(),
        }
    }

    /// Cache de respostas num arquivo SQLite próprio (descartável).
    pub fn cache_em(mut self, caminho: impl AsRef<Path>) -> Self {
        self.cache = LugarDoCache::Arquivo(caminho.as_ref().to_path_buf());
        self
    }

    /// Cache de respostas em memória (testes).
    pub fn cache_em_memoria(mut self) -> Self {
        self.cache = LugarDoCache::Memoria;
        self
    }

    pub fn relogio(mut self, relogio: Arc<dyn Relogio>) -> Self {
        self.relogio = relogio;
        self
    }

    /// Manda o host oficial `autoridade` (`host` ou `host:porta`) para `base` (servidor falso,
    /// servidor local de teste). Só HTTPS ou loopback (`http://127.0.0.1`, `http://localhost`,
    /// `http://[::1]`); a origem, o catálogo e o cache continuam usando o endereço oficial.
    pub fn rota(mut self, autoridade: &str, base: &str) -> Self {
        self.rotas.push((
            autoridade.to_ascii_lowercase(),
            base.trim_end_matches('/').to_owned(),
        ));
        self
    }

    /// Ritmo próprio para um host (`host[:porta]`).
    pub fn ritmo(mut self, autoridade: &str, ritmo: Ritmo) -> Self {
        self.ritmos.insert(autoridade.to_ascii_lowercase(), ritmo);
        self
    }

    pub fn ritmo_padrao(mut self, ritmo: Ritmo) -> Self {
        self.ritmo_padrao = ritmo;
        self
    }

    pub fn repeticao(mut self, politica: PoliticaDeRepeticao) -> Self {
        self.repeticao = politica;
        self
    }

    /// De onde vem o `Authorization` (o chaveiro do app).
    pub fn autorizacao(mut self, fonte: Arc<dyn FonteDeAutorizacao>) -> Self {
        self.autorizacao = fonte;
        self
    }

    /// Prazo padrão de cada pedido.
    pub fn prazo(mut self, prazo: Duration) -> Self {
        self.prazo = prazo;
        self
    }

    pub fn prazo_de_conexao(mut self, prazo: Duration) -> Self {
        self.prazo_de_conexao = prazo;
        self
    }

    /// Limite padrão de bytes de uma resposta.
    pub fn limite_de_corpo(mut self, bytes: usize) -> Self {
        self.limite_de_corpo = bytes;
        self
    }

    /// Semente do jitter do backoff (testes reprodutíveis; o padrão é aleatório).
    pub fn semente(mut self, semente: u64) -> Self {
        self.semente = Some(semente);
        self
    }

    /// `User-Agent` (o nome e a versão do app; nenhum dado do usuário).
    pub fn agente_de_usuario(mut self, agente: impl Into<String>) -> Self {
        self.agente_de_usuario = agente.into();
        self
    }
}

fn rota_valida(base: &str) -> bool {
    let loopback = ["http://127.0.0.1", "http://localhost", "http://[::1]"]
        .iter()
        .any(|prefixo| {
            base.strip_prefix(prefixo)
                .is_some_and(|resto| resto.is_empty() || resto.starts_with(':'))
        });
    loopback || base.starts_with("https://")
}

fn travar<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Debug)]
struct EstadoDoHost {
    balde: Balde,
    limitado_ate: Option<Instante>,
    falhando: Option<(Instante, ErroDeRede)>,
}

/// Pedido admitido pelo portão.
#[derive(Debug)]
struct Admitido {
    alvo: Alvo,
    endereco: Endereco,
    autorizacao: Option<Autorizacao>,
}

struct Trabalho {
    pedido: Pedido,
    cancelar: Arc<AtomicBool>,
    resposta: Sender<Result<Resposta, ErroDeRede>>,
}

struct Compartilhado {
    relogio: Arc<dyn Relogio>,
    http: Http,
    cache: Option<Mutex<Cache>>,
    catalogo: Catalogo,
    rotas: HashMap<String, String>,
    ritmos: HashMap<String, Ritmo>,
    ritmo_padrao: Ritmo,
    repeticao: PoliticaDeRepeticao,
    autorizacao: Arc<dyn FonteDeAutorizacao>,
    prazo: Duration,
    limite_de_corpo: usize,
    sorteio: Mutex<SplitMix64>,
    hosts: Mutex<HashMap<String, EstadoDoHost>>,
    operacoes: Mutex<VecDeque<Operacao>>,
    ultimo_contato: Mutex<Option<Instante>>,
    ativos: AtomicUsize,
}

struct Interno {
    compartilhado: Arc<Compartilhado>,
    filas: Mutex<HashMap<String, Sender<Trabalho>>>,
}

/// O portão de rede. Barato de clonar: os clones compartilham filas, limites e cache.
#[derive(Clone)]
pub struct Rede {
    interno: Arc<Interno>,
}

impl fmt::Debug for Rede {
    /// Só contagens: nada da credencial, das rotas ou do cache.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let c = &self.interno.compartilhado;
        f.debug_struct("Rede")
            .field("enderecos", &c.catalogo.enderecos().len())
            .field("pedidos_ativos", &c.ativos.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl Rede {
    /// Abre o portão.
    ///
    /// # Errors
    /// Rota que não é HTTPS nem loopback, ou falha ao abrir o cache.
    pub fn abrir(configuracao: ConfiguracaoDaRede) -> Result<Self, ErroDeRede> {
        let cache = match &configuracao.cache {
            LugarDoCache::Nenhum => None,
            LugarDoCache::Memoria => Some(Mutex::new(Cache::em_memoria()?)),
            LugarDoCache::Arquivo(caminho) => Some(Mutex::new(Cache::abrir(caminho)?)),
        };
        let mut rotas = HashMap::new();
        for (autoridade, base) in &configuracao.rotas {
            if !rota_valida(base) {
                return Err(ErroDeRede::invalido(format!(
                    "a rota de {autoridade} precisa ser HTTPS ou loopback"
                )));
            }
            rotas.insert(autoridade.clone(), base.clone());
        }
        let semente = configuracao.semente.unwrap_or_else(|| {
            let mut h = RandomState::new().build_hasher();
            h.write_u64(0x5eed);
            h.finish()
        });
        let compartilhado = Compartilhado {
            relogio: configuracao.relogio,
            http: Http::novo(
                configuracao.prazo_de_conexao,
                &configuracao.agente_de_usuario,
            ),
            cache,
            catalogo: configuracao.catalogo,
            rotas,
            ritmos: configuracao.ritmos,
            ritmo_padrao: configuracao.ritmo_padrao,
            repeticao: configuracao.repeticao,
            autorizacao: configuracao.autorizacao,
            prazo: configuracao.prazo,
            limite_de_corpo: configuracao.limite_de_corpo,
            sorteio: Mutex::new(SplitMix64::novo(semente)),
            hosts: Mutex::new(HashMap::new()),
            operacoes: Mutex::new(VecDeque::new()),
            ultimo_contato: Mutex::new(None),
            ativos: AtomicUsize::new(0),
        };
        Ok(Self {
            interno: Arc::new(Interno {
                compartilhado: Arc::new(compartilhado),
                filas: Mutex::new(HashMap::new()),
            }),
        })
    }

    pub fn catalogo(&self) -> &Catalogo {
        &self.interno.compartilhado.catalogo
    }

    /// O relógio do portão (o mesmo que carimba as respostas).
    pub fn relogio(&self) -> &Arc<dyn Relogio> {
        &self.interno.compartilhado.relogio
    }

    /// Envia o pedido pela fila do host. Nunca bloqueia: um pedido recusado pelo portão
    /// (fora do catálogo, sem credencial, mal montado) já volta resolvido, sem thread e sem
    /// rede.
    pub fn buscar(&self, pedido: Pedido) -> Pendente {
        let c = &self.interno.compartilhado;
        let inicio = c.relogio.agora();
        let admitido = match c.admitir(&pedido) {
            Ok(a) => a,
            Err(erro) => {
                c.registrar(&pedido, inicio, &Err(erro.clone()));
                return Pendente::pronto(Err(erro));
            }
        };
        let (resposta, rx) = mpsc::channel();
        let cancelar = Arc::new(AtomicBool::new(false));
        let pendente = Pendente {
            rx,
            cancelar: cancelar.clone(),
            terminado: false,
        };
        c.ativos.fetch_add(1, Ordering::AcqRel);
        let trabalho = Trabalho {
            pedido,
            cancelar,
            resposta,
        };
        if self
            .fila(&admitido.endereco.autoridade())
            .send(trabalho)
            .is_err()
        {
            c.ativos.fetch_sub(1, Ordering::AcqRel);
            return Pendente::pronto(Err(ErroDeRede::SemRede {
                detalhe: "fila do host indisponível",
            }));
        }
        pendente
    }

    /// Roda o pedido pelo portão **na thread de quem chama**, bloqueando até o fim. Para a
    /// sincronização e outros trabalhos que já estão fora da thread da interface.
    ///
    /// # Errors
    /// Qualquer [`ErroDeRede`].
    pub fn executar(&self, pedido: &Pedido) -> Result<Resposta, ErroDeRede> {
        self.executar_cancelavel(pedido, &AtomicBool::new(false))
    }

    /// Como [`Rede::executar`], desistindo quando `cancelar` for acionado.
    ///
    /// # Errors
    /// Qualquer [`ErroDeRede`] ([`ErroDeRede::Cancelado`] ao desistir).
    pub fn executar_cancelavel(
        &self,
        pedido: &Pedido,
        cancelar: &AtomicBool,
    ) -> Result<Resposta, ErroDeRede> {
        let c = &self.interno.compartilhado;
        let inicio = c.relogio.agora();
        c.ativos.fetch_add(1, Ordering::AcqRel);
        let resultado = c.rodar(pedido, cancelar);
        c.ativos.fetch_sub(1, Ordering::AcqRel);
        c.registrar(pedido, inicio, &resultado);
        resultado
    }

    /// Lê só o cache do portão, sem rede (leitura offline do que já está no disco). A idade
    /// diz quão antigo é o conteúdo.
    ///
    /// # Errors
    /// Pedido mal montado ou falha do cache.
    pub fn ler_cache(&self, pedido: &Pedido) -> Result<Option<Resposta>, ErroDeRede> {
        let c = &self.interno.compartilhado;
        let Some(cache) = &c.cache else {
            return Ok(None);
        };
        let alvo = pedido.alvo()?;
        let guardada = travar(cache).ler(&pedido.chave_do_cache(&alvo))?;
        Ok(guardada.map(|g| de_guardada(g, c.relogio.agora(), Frescor::Cache)))
    }

    /// Apaga o cache inteiro do portão; devolve quantas respostas saíram.
    ///
    /// # Errors
    /// Falha do cache.
    pub fn limpar_cache(&self) -> Result<usize, ErroDeRede> {
        match &self.interno.compartilhado.cache {
            Some(cache) => travar(cache).limpar(),
            None => Ok(0),
        }
    }

    /// Estado da rede para a barra de status (barato; pode ser lido a cada quadro).
    pub fn estado(&self) -> EstadoDaRede {
        let c = &self.interno.compartilhado;
        let agora = c.relogio.agora();
        let hosts = travar(&c.hosts);
        let mut vistos = Vec::new();
        let lista = c
            .catalogo
            .enderecos()
            .iter()
            .map(Endereco::autoridade)
            .filter(|a| {
                let novo = !vistos.contains(a);
                if novo {
                    vistos.push(a.clone());
                }
                novo
            })
            .map(|autoridade| {
                let situacao = match hosts.get(&autoridade) {
                    Some(EstadoDoHost {
                        limitado_ate: Some(ate),
                        ..
                    }) if *ate > agora => SituacaoDoHost::Limitado { ate: *ate },
                    Some(EstadoDoHost {
                        falhando: Some((desde, erro)),
                        ..
                    }) => SituacaoDoHost::Falhando {
                        desde: *desde,
                        erro: erro.clone(),
                    },
                    _ => SituacaoDoHost::Disponivel,
                };
                (autoridade, situacao)
            })
            .collect();
        EstadoDaRede {
            hosts: lista,
            pedidos_ativos: c.ativos.load(Ordering::Acquire),
            ultimo_contato: *travar(&c.ultimo_contato),
        }
    }

    /// Últimas operações (sanitizadas), da mais antiga à mais nova.
    pub fn operacoes_recentes(&self) -> Vec<Operacao> {
        travar(&self.interno.compartilhado.operacoes)
            .iter()
            .cloned()
            .collect()
    }

    fn fila(&self, autoridade: &str) -> Sender<Trabalho> {
        let mut filas = travar(&self.interno.filas);
        filas
            .entry(autoridade.to_owned())
            .or_insert_with(|| {
                let (tx, rx) = mpsc::channel::<Trabalho>();
                let c = self.interno.compartilhado.clone();
                let nome = format!("bancada-rede-{autoridade}");
                // Sem a thread, o `rx` cai e o `send` falha: o pedido volta com erro.
                let _ = thread::Builder::new()
                    .name(nome)
                    .spawn(move || trabalhador(&c, &rx));
                tx
            })
            .clone()
    }
}

fn trabalhador(c: &Compartilhado, trabalhos: &Receiver<Trabalho>) {
    while let Ok(trabalho) = trabalhos.recv() {
        if !trabalho.cancelar.load(Ordering::Acquire) {
            let inicio = c.relogio.agora();
            let resultado = c.rodar(&trabalho.pedido, &trabalho.cancelar);
            c.registrar(&trabalho.pedido, inicio, &resultado);
            if !trabalho.cancelar.load(Ordering::Acquire) {
                let _ = trabalho.resposta.send(resultado);
            }
        }
        c.ativos.fetch_sub(1, Ordering::AcqRel);
    }
}

fn de_guardada(g: Guardada, agora: Instante, frescor: Frescor) -> Resposta {
    Resposta {
        status: g.status,
        tipo: g.tipo,
        corpo: g.corpo,
        etag: g.etag,
        origem: g.origem,
        consultada_em: g.consultada_em,
        idade: agora.desde(g.consultada_em),
        frescor,
    }
}

impl Compartilhado {
    /// O portão: alvo HTTPS, catálogo e credencial, nesta ordem. Chamado ao enviar e de novo
    /// logo antes de cada ida à rede (a credencial pode ter mudado na espera).
    fn admitir(&self, pedido: &Pedido) -> Result<Admitido, ErroDeRede> {
        let alvo = pedido.alvo()?;
        let endereco = self.catalogo.endereco_para(&alvo).cloned().ok_or_else(|| {
            ErroDeRede::EnderecoForaDoCatalogo {
                endereco: sanitizado(&pedido.url),
            }
        })?;
        let autorizacao = if pedido.autorizado {
            if !endereco.recebe_autorizacao() {
                return Err(ErroDeRede::EnderecoSemAutorizacao {
                    endereco: endereco.nome().to_owned(),
                });
            }
            Some(self.autorizacao.autorizacao(&endereco).ok_or_else(|| {
                ErroDeRede::SemCredencial {
                    endereco: endereco.nome().to_owned(),
                }
            })?)
        } else {
            None
        };
        Ok(Admitido {
            alvo,
            endereco,
            autorizacao,
        })
    }

    fn host<R>(&self, autoridade: &str, f: impl FnOnce(&mut EstadoDoHost) -> R) -> R {
        let mut mapa = travar(&self.hosts);
        let ritmo = self
            .ritmos
            .get(autoridade)
            .copied()
            .unwrap_or(self.ritmo_padrao);
        let estado = mapa
            .entry(autoridade.to_owned())
            .or_insert_with(|| EstadoDoHost {
                balde: Balde::novo(ritmo),
                limitado_ate: None,
                falhando: None,
            });
        f(estado)
    }

    fn respondeu(&self, autoridade: &str, agora: Instante) {
        self.host(autoridade, |h| h.falhando = None);
        *travar(&self.ultimo_contato) = Some(agora);
    }

    fn falhou(&self, autoridade: &str, agora: Instante, erro: &ErroDeRede) {
        self.host(autoridade, |h| {
            let desde = h.falhando.as_ref().map_or(agora, |(d, _)| *d);
            h.falhando = Some((desde, erro.clone()));
        });
    }

    fn registrar(
        &self,
        pedido: &Pedido,
        inicio: Instante,
        resultado: &Result<Resposta, ErroDeRede>,
    ) {
        let (host, caminho) = match pedido.alvo() {
            Ok(alvo) => (alvo.autoridade(), alvo.caminho),
            Err(_) => (String::new(), String::new()),
        };
        let operacao = Operacao {
            metodo: pedido.metodo,
            host,
            caminho,
            inicio,
            fim: self.relogio.agora(),
            status_http: match resultado {
                Ok(r) => Some(r.status),
                Err(e) => e.status_http(),
            },
            resultado: resultado
                .as_ref()
                .map(|r| r.frescor.clone())
                .map_err(Clone::clone),
        };
        let mut log = travar(&self.operacoes);
        if log.len() == OPERACOES_GUARDADAS {
            log.pop_front();
        }
        log.push_back(operacao);
    }

    fn url_roteada(&self, pedido: &Pedido, alvo: &Alvo) -> String {
        let base = self
            .rotas
            .get(&alvo.autoridade())
            .cloned()
            .unwrap_or_else(|| format!("https://{}", alvo.autoridade()));
        let consulta = pedido.consulta_codificada();
        if consulta.is_empty() {
            format!("{base}{}", alvo.caminho)
        } else {
            format!("{base}{}?{consulta}", alvo.caminho)
        }
    }

    fn vencido_ou(
        &self,
        pedido: &Pedido,
        guardada: Option<&Guardada>,
        causa: ErroDeRede,
    ) -> Result<Resposta, ErroDeRede> {
        match guardada {
            Some(g) if pedido.aceitar_vencido => Ok(de_guardada(
                g.clone(),
                self.relogio.agora(),
                Frescor::Vencida { causa },
            )),
            _ => Err(causa),
        }
    }

    /// Executa um pedido: portão, cache, limites, rede, repetições.
    fn rodar(&self, pedido: &Pedido, cancelar: &AtomicBool) -> Result<Resposta, ErroDeRede> {
        let mut admitido = self.admitir(pedido)?;
        let autoridade = admitido.endereco.autoridade();
        let chave = pedido.chave_do_cache(&admitido.alvo);
        let origem = pedido.url_canonica(&admitido.alvo);
        let guardada = match (&self.cache, pedido.guardar_por) {
            (Some(cache), Some(_)) => travar(cache).ler(&chave)?,
            _ => None,
        };
        if let Some(g) = &guardada
            && !pedido.renovar
            && g.esta_fresca(self.relogio.agora())
        {
            return Ok(de_guardada(g.clone(), self.relogio.agora(), Frescor::Cache));
        }

        let mut repeticoes = 0u32;
        let mut pausas = 0u32;
        loop {
            let agora = self.relogio.agora();
            let limitado = self.host(&autoridade, |h| h.limitado_ate.filter(|a| *a > agora));
            if let Some(ate) = limitado {
                if ate.desde(agora) > pedido.paciencia || pausas >= MAXIMO_DE_PAUSAS {
                    return self.vencido_ou(
                        pedido,
                        guardada.as_ref(),
                        ErroDeRede::Limitado { ate },
                    );
                }
                if !self.relogio.dormir_ate(ate, cancelar) {
                    return Err(ErroDeRede::Cancelado);
                }
            }
            let vez = self.host(&autoridade, |h| h.balde.reservar(self.relogio.agora()));
            if vez > self.relogio.agora() && !self.relogio.dormir_ate(vez, cancelar) {
                return Err(ErroDeRede::Cancelado);
            }
            // A credencial pode ter mudado (ou sido apagada) durante a espera.
            admitido = self.admitir(pedido)?;
            let etag = guardada
                .as_ref()
                .and_then(|g| g.etag.clone())
                .or_else(|| pedido.etag_conhecida.clone());
            let mut cabecalhos = pedido.cabecalhos.clone();
            if let Some(etag) = etag {
                cabecalhos.push(("if-none-match".to_owned(), etag));
            }
            let saida = Saida {
                metodo: pedido.metodo,
                url: self.url_roteada(pedido, &admitido.alvo),
                cabecalhos,
                autorizacao: admitido.autorizacao.as_ref().map(Autorizacao::valor),
                corpo: pedido.corpo.as_ref(),
                prazo: pedido.prazo.unwrap_or(self.prazo),
                limite_de_corpo: pedido.limite_de_corpo.unwrap_or(self.limite_de_corpo),
                cancelar,
            };
            let resultado = self.http.enviar(&saida);
            let agora = self.relogio.agora();
            let (cabeca, corpo) = match resultado {
                Ok(r) => r,
                Err(Falha::Cancelado) => return Err(ErroDeRede::Cancelado),
                Err(Falha::CorpoGrandeDemais) => {
                    return Err(ErroDeRede::CorpoGrandeDemais {
                        limite: saida.limite_de_corpo,
                    });
                }
                Err(Falha::Prazo) => {
                    let erro = ErroDeRede::PrazoEsgotado;
                    self.falhou(&autoridade, agora, &erro);
                    return self.vencido_ou(pedido, guardada.as_ref(), erro);
                }
                Err(Falha::Inalcancavel(detalhe)) => {
                    let erro = ErroDeRede::SemRede { detalhe };
                    self.falhou(&autoridade, agora, &erro);
                    return self.vencido_ou(pedido, guardada.as_ref(), erro);
                }
                Err(Falha::Interrompida(detalhe)) => {
                    if self.esperar_repeticao(pedido, &mut repeticoes, None, cancelar)? {
                        continue;
                    }
                    let erro = ErroDeRede::Interrompida { detalhe };
                    self.falhou(&autoridade, agora, &erro);
                    return self.vencido_ou(pedido, guardada.as_ref(), erro);
                }
            };
            match cabeca.status {
                200..=299 => {
                    self.respondeu(&autoridade, agora);
                    return self.nova(pedido, &chave, origem, cabeca, corpo);
                }
                304 => {
                    self.respondeu(&autoridade, agora);
                    if let (Some(g), Some(cache)) = (guardada, &self.cache) {
                        let vence = agora.mais(pedido.guardar_por.unwrap_or_default());
                        travar(cache).renovar(&chave, agora, vence, cabeca.etag.as_deref())?;
                        let renovada = Guardada {
                            consultada_em: agora,
                            vence_em: vence,
                            ..g
                        };
                        return Ok(de_guardada(renovada, agora, Frescor::Revalidada));
                    }
                    if pedido.etag_conhecida.is_some() {
                        return Ok(Resposta {
                            status: 304,
                            tipo: None,
                            corpo: Vec::new(),
                            etag: cabeca.etag.or_else(|| pedido.etag_conhecida.clone()),
                            origem,
                            consultada_em: agora,
                            idade: Duration::ZERO,
                            frescor: Frescor::NaoMudou,
                        });
                    }
                    return Err(ErroDeRede::Http {
                        status: 304,
                        corpo: CorpoDeErro::default(),
                    });
                }
                429 => {
                    self.respondeu(&autoridade, agora);
                    let ate = cabeca
                        .tentar_de_novo
                        .as_deref()
                        .and_then(|v| tentar_de_novo_em(v, agora))
                        .unwrap_or_else(|| agora.mais(PAUSA_PADRAO));
                    self.host(&autoridade, |h| {
                        h.limitado_ate = Some(h.limitado_ate.map_or(ate, |a| a.max(ate)));
                    });
                    pausas += 1;
                }
                500 | 502 | 503 | 504 => {
                    let erro = ErroDeRede::Http {
                        status: cabeca.status,
                        corpo: CorpoDeErro::novo(corpo),
                    };
                    let dica = cabeca
                        .tentar_de_novo
                        .as_deref()
                        .and_then(|v| tentar_de_novo_em(v, agora));
                    if self.esperar_repeticao(pedido, &mut repeticoes, dica, cancelar)? {
                        continue;
                    }
                    *travar(&self.ultimo_contato) = Some(agora);
                    self.falhou(&autoridade, agora, &erro);
                    return self.vencido_ou(pedido, guardada.as_ref(), erro);
                }
                300..=399 => {
                    self.respondeu(&autoridade, agora);
                    return Err(ErroDeRede::RedirecionamentoRecusado {
                        status: cabeca.status,
                    });
                }
                status @ (401 | 403) if admitido.autorizacao.is_some() => {
                    self.respondeu(&autoridade, agora);
                    let corpo = CorpoDeErro::novo(corpo);
                    self.autorizacao
                        .recusada(&admitido.endereco, status, &corpo);
                    return Err(ErroDeRede::AutorizacaoRecusada { status, corpo });
                }
                status => {
                    // O servidor respondeu: está alcançável, o erro é do pedido.
                    self.respondeu(&autoridade, agora);
                    return Err(ErroDeRede::Http {
                        status,
                        corpo: CorpoDeErro::novo(corpo),
                    });
                }
            }
        }
    }

    /// Espera a próxima repetição de uma falha temporária; `Ok(false)` se não há mais
    /// repetições ou o método não é idempotente (`POST` nunca se repete sozinho).
    fn esperar_repeticao(
        &self,
        pedido: &Pedido,
        repeticoes: &mut u32,
        dica: Option<Instante>,
        cancelar: &AtomicBool,
    ) -> Result<bool, ErroDeRede> {
        if pedido.metodo != Metodo::Get || *repeticoes >= self.repeticao.repeticoes {
            return Ok(false);
        }
        let espera = self
            .repeticao
            .espera(*repeticoes, &mut travar(&self.sorteio));
        *repeticoes += 1;
        let agora = self.relogio.agora();
        let ate = dica.map_or(agora.mais(espera), |d| d.max(agora.mais(espera)));
        if self.relogio.dormir_ate(ate, cancelar) {
            Ok(true)
        } else {
            Err(ErroDeRede::Cancelado)
        }
    }

    fn nova(
        &self,
        pedido: &Pedido,
        chave: &str,
        origem: String,
        cabeca: Cabeca,
        corpo: Vec<u8>,
    ) -> Result<Resposta, ErroDeRede> {
        let agora = self.relogio.agora();
        if let (Some(cache), Some(validade)) = (&self.cache, pedido.guardar_por)
            && cabeca.status == 200
        {
            travar(cache).gravar(
                chave,
                &Guardada {
                    origem: origem.clone(),
                    status: cabeca.status,
                    tipo: cabeca.tipo.clone(),
                    corpo: corpo.clone(),
                    etag: cabeca.etag.clone(),
                    consultada_em: agora,
                    vence_em: agora.mais(validade),
                },
            )?;
        }
        Ok(Resposta {
            status: cabeca.status,
            tipo: cabeca.tipo,
            corpo,
            etag: cabeca.etag,
            origem,
            consultada_em: agora,
            idade: Duration::ZERO,
            frescor: Frescor::Rede,
        })
    }
}

/// Um pedido em andamento. A interface consulta [`Pendente::tentar_pegar`] a cada quadro;
/// descartar o `Pendente` cancela o pedido.
#[derive(Debug)]
pub struct Pendente {
    rx: Receiver<Result<Resposta, ErroDeRede>>,
    cancelar: Arc<AtomicBool>,
    terminado: bool,
}

impl Pendente {
    fn pronto(resultado: Result<Resposta, ErroDeRede>) -> Self {
        let (tx, rx) = mpsc::channel();
        let _ = tx.send(resultado);
        Self {
            rx,
            cancelar: Arc::new(AtomicBool::new(false)),
            terminado: false,
        }
    }

    /// O resultado, se já chegou (nunca bloqueia). Depois de devolver `Some`, devolve `None`.
    pub fn tentar_pegar(&mut self) -> Option<Result<Resposta, ErroDeRede>> {
        if self.terminado {
            return None;
        }
        match self.rx.try_recv() {
            Ok(resultado) => {
                self.terminado = true;
                Some(resultado)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.terminado = true;
                Some(Err(ErroDeRede::Cancelado))
            }
        }
    }

    /// Espera o resultado (bloqueia: fora da thread da interface).
    ///
    /// # Errors
    /// O erro do pedido.
    pub fn esperar(mut self) -> Result<Resposta, ErroDeRede> {
        let resultado = self.rx.recv().unwrap_or(Err(ErroDeRede::Cancelado));
        self.terminado = true;
        resultado
    }

    /// Desiste do pedido (a thread larga na próxima conferência).
    pub fn cancelar(&self) {
        self.cancelar.store(true, Ordering::Release);
    }
}

impl Drop for Pendente {
    fn drop(&mut self) {
        if !self.terminado {
            self.cancelar.store(true, Ordering::Release);
        }
    }
}
