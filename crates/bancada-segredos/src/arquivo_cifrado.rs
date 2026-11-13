//! O envelope do arquivo cifrado: cabeçalho, derivação da chave pela senha-mestra (Argon2id) e
//! cifra autenticada (XChaCha20-Poly1305). O formato está descrito na documentação da crate
//! (`lib.rs`); o que vai dentro do envelope é do módulo `conteudo`.
//!
//! O cabeçalho é o mesmo nos formatos 1 (chess-vault) e 2 (bancada): muda só o que o conteúdo
//! decifrado significa. A versão está no cabeçalho, e o cabeçalho inteiro é o dado associado
//! (AAD) da cifra, então ninguém troca a versão sem a leitura falhar.

use crate::erro::ErroDeSegredo;
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{AeadInOut, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use zeroize::{Zeroize, Zeroizing};

pub(crate) const ASSINATURA: &[u8; 8] = b"CSVAULT\0";
/// Formato do chess-vault: conteúdo em JSON, só lido.
pub(crate) const VERSAO_1: u16 = 1;
/// Formato da bancada: conteúdo binário com rótulos de texto. É o que se grava.
pub(crate) const VERSAO_2: u16 = 2;
const KDF_ARGON2ID: u8 = 1;
const CIFRA_XCHACHA20POLY1305: u8 = 1;
const TAMANHO_DO_SAL: usize = 16;
const TAMANHO_DO_VERIFICADOR: usize = 32;
const TAMANHO_DA_CHAVE: usize = 32;
const TAMANHO_DO_NONCE: usize = 24;
const TAMANHO_DA_ETIQUETA: usize = 16;
pub(crate) const TAMANHO_DO_CABECALHO: usize =
    8 + 2 + 1 + 12 + TAMANHO_DO_SAL + TAMANHO_DO_VERIFICADOR + 1 + TAMANHO_DO_NONCE;

/// Limites aceitos ao **ler**: um cabeçalho com custo absurdo (arquivo corrompido ou forjado)
/// não trava o app tentando derivar a chave. Os mesmos do chess-vault.
const MAXIMO_DE_MEMORIA_KIB: u32 = 1024 * 1024;
const MAXIMO_DE_ITERACOES: u32 = 64;
const MAXIMO_DE_PARALELISMO: u32 = 16;

/// Mínimos aceitos ao **gravar** (recomendação mínima da OWASP para o Argon2id: 19 MiB e 2
/// passadas). A leitura aceita custos menores, para abrir arquivos antigos e fixtures.
const MINIMO_DE_MEMORIA_KIB: u32 = 19 * 1024;
const MINIMO_DE_ITERACOES: u32 = 2;

/// Custo do Argon2id para criar o arquivo ou trocar a senha-mestra (ao abrir, vale o que está
/// no arquivo). O padrão segue a RFC 9106 para memória limitada: 64 MiB, 3 passadas, 1 via.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParametrosDeDerivacao {
    memoria_kib: u32,
    iteracoes: u32,
    paralelismo: u32,
}

impl Default for ParametrosDeDerivacao {
    fn default() -> Self {
        Self {
            memoria_kib: 64 * 1024,
            iteracoes: 3,
            paralelismo: 1,
        }
    }
}

impl ParametrosDeDerivacao {
    /// Custo próprio, dentro dos limites: pelo menos 19 MiB e 2 passadas, no máximo 1 GiB, 64
    /// passadas e 16 vias, e memória de pelo menos 8 KiB por via.
    pub fn novos(
        memoria_kib: u32,
        iteracoes: u32,
        paralelismo: u32,
    ) -> Result<Self, ErroDeSegredo> {
        let parametros = Self {
            memoria_kib,
            iteracoes,
            paralelismo,
        };
        if parametros.legiveis()
            && memoria_kib >= MINIMO_DE_MEMORIA_KIB
            && iteracoes >= MINIMO_DE_ITERACOES
        {
            Ok(parametros)
        } else {
            Err(ErroDeSegredo::ParametrosInvalidos)
        }
    }

    /// Custo baixíssimo (64 KiB, 1 passada), só para a suíte de testes correr.
    #[cfg(any(test, feature = "testes"))]
    pub const fn rapidos_para_testes() -> Self {
        Self {
            memoria_kib: 64,
            iteracoes: 1,
            paralelismo: 1,
        }
    }

    pub fn memoria_kib(&self) -> u32 {
        self.memoria_kib
    }

    pub fn iteracoes(&self) -> u32 {
        self.iteracoes
    }

    pub fn paralelismo(&self) -> u32 {
        self.paralelismo
    }

    /// Dentro do que a leitura aceita.
    fn legiveis(&self) -> bool {
        (1..=MAXIMO_DE_MEMORIA_KIB).contains(&self.memoria_kib)
            && (1..=MAXIMO_DE_ITERACOES).contains(&self.iteracoes)
            && (1..=MAXIMO_DE_PARALELISMO).contains(&self.paralelismo)
            && self.memoria_kib >= 8 * self.paralelismo
    }
}

/// Chave da cifra, zerada ao descartar.
pub(crate) type Chave = Zeroizing<[u8; TAMANHO_DA_CHAVE]>;

/// O que identifica a derivação: parâmetros, sal e verificador. Fica em memória com o arquivo
/// aberto, para regravar com a mesma chave (cada gravação tem nonce novo).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Derivacao {
    parametros: ParametrosDeDerivacao,
    sal: [u8; TAMANHO_DO_SAL],
    verificador: [u8; TAMANHO_DO_VERIFICADOR],
}

fn aleatorio<const N: usize>() -> Result<[u8; N], ErroDeSegredo> {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes).map_err(|_| ErroDeSegredo::Interno {
        detalhe: "gerador aleatório do sistema indisponível",
    })?;
    Ok(bytes)
}

/// Argon2id da senha com o sal: 64 bytes, os 32 primeiros são o verificador e os 32 últimos, a
/// chave. Qualquer falha aqui (parâmetros que o Argon2 recusa) é tratada como arquivo inválido.
fn argon2id(
    parametros: &ParametrosDeDerivacao,
    senha: &[u8],
    sal: &[u8],
) -> Result<(Zeroizing<[u8; TAMANHO_DO_VERIFICADOR]>, Chave), ErroDeSegredo> {
    let invalido = ErroDeSegredo::SenhaOuArquivoInvalido;
    let p = Params::new(
        parametros.memoria_kib,
        parametros.iteracoes,
        parametros.paralelismo,
        Some(TAMANHO_DO_VERIFICADOR + TAMANHO_DA_CHAVE),
    )
    .map_err(|_| invalido.clone())?;
    let mut saida = Zeroizing::new([0u8; TAMANHO_DO_VERIFICADOR + TAMANHO_DA_CHAVE]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, p)
        .hash_password_into(senha, sal, saida.as_mut())
        .map_err(|_| invalido)?;
    let mut verificador = Zeroizing::new([0u8; TAMANHO_DO_VERIFICADOR]);
    verificador.copy_from_slice(&saida[..TAMANHO_DO_VERIFICADOR]);
    let mut chave = Zeroizing::new([0u8; TAMANHO_DA_CHAVE]);
    chave.copy_from_slice(&saida[TAMANHO_DO_VERIFICADOR..]);
    Ok((verificador, chave))
}

/// Derivação nova (sal novo) para `senha`: criar o arquivo e trocar a senha-mestra.
pub(crate) fn derivar_nova(
    parametros: ParametrosDeDerivacao,
    senha: &[u8],
) -> Result<(Derivacao, Chave), ErroDeSegredo> {
    let sal = aleatorio::<TAMANHO_DO_SAL>()?;
    let (verificador, chave) = argon2id(&parametros, senha, &sal)?;
    Ok((
        Derivacao {
            parametros,
            sal,
            verificador: *verificador,
        },
        chave,
    ))
}

/// Comparação em tempo constante.
fn iguais(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn cabecalho(versao: u16, derivacao: &Derivacao, nonce: &[u8; TAMANHO_DO_NONCE]) -> Vec<u8> {
    let mut saida = Vec::with_capacity(TAMANHO_DO_CABECALHO);
    saida.extend_from_slice(ASSINATURA);
    saida.extend_from_slice(&versao.to_le_bytes());
    saida.push(KDF_ARGON2ID);
    saida.extend_from_slice(&derivacao.parametros.memoria_kib.to_le_bytes());
    saida.extend_from_slice(&derivacao.parametros.iteracoes.to_le_bytes());
    saida.extend_from_slice(&derivacao.parametros.paralelismo.to_le_bytes());
    saida.extend_from_slice(&derivacao.sal);
    saida.extend_from_slice(&derivacao.verificador);
    saida.push(CIFRA_XCHACHA20POLY1305);
    saida.extend_from_slice(nonce);
    saida
}

fn cifra(chave: &Chave) -> Result<XChaCha20Poly1305, ErroDeSegredo> {
    XChaCha20Poly1305::new_from_slice(chave.as_ref()).map_err(|_| ErroDeSegredo::Interno {
        detalhe: "chave da cifra inválida",
    })
}

/// Cifra `conteudo` num arquivo completo do formato 2 (cabeçalho + texto cifrado + etiqueta),
/// com nonce novo.
pub(crate) fn selar(
    derivacao: &Derivacao,
    chave: &Chave,
    conteudo: &[u8],
) -> Result<Vec<u8>, ErroDeSegredo> {
    let nonce = aleatorio::<TAMANHO_DO_NONCE>()?;
    let mut arquivo = cabecalho(VERSAO_2, derivacao, &nonce);
    // O buffer recebe o conteúdo em claro e vira texto cifrado no lugar; é zerado ao sair de
    // qualquer jeito (inclusive se a cifra falhar).
    let mut buffer = Zeroizing::new(conteudo.to_vec());
    let etiqueta = cifra(chave)?
        .encrypt_inout_detached(&XNonce::from(nonce), &arquivo, buffer.as_mut_slice().into())
        .map_err(|_| ErroDeSegredo::Interno {
            detalhe: "falha ao cifrar",
        })?;
    arquivo.reserve_exact(buffer.len() + TAMANHO_DA_ETIQUETA);
    arquivo.extend_from_slice(&buffer);
    arquivo.extend_from_slice(&etiqueta);
    Ok(arquivo)
}

fn u32_em(bytes: &[u8], em: usize) -> u32 {
    let mut b = [0u8; 4];
    b.copy_from_slice(&bytes[em..em + 4]);
    u32::from_le_bytes(b)
}

/// Um arquivo cuja estrutura foi conferida, antes da senha.
#[derive(Debug)]
pub(crate) struct ArquivoCifrado<'a> {
    versao: u16,
    derivacao: Derivacao,
    nonce: [u8; TAMANHO_DO_NONCE],
    aad: &'a [u8],
    corpo: &'a [u8],
}

/// O que sai de um arquivo destrancado.
pub(crate) struct Aberto {
    pub(crate) versao: u16,
    pub(crate) derivacao: Derivacao,
    pub(crate) chave: Chave,
    pub(crate) conteudo: Zeroizing<Vec<u8>>,
}

impl<'a> ArquivoCifrado<'a> {
    /// Confere a estrutura (sem a senha). Tudo o que não é um arquivo íntegro de versão
    /// conhecida é [`ErroDeSegredo::SenhaOuArquivoInvalido`], exceto versão mais nova, que é
    /// [`ErroDeSegredo::FormatoNaoSuportado`].
    pub(crate) fn ler(arquivo: &'a [u8]) -> Result<Self, ErroDeSegredo> {
        let invalido = ErroDeSegredo::SenhaOuArquivoInvalido;
        if arquivo.len() < 10 || &arquivo[..8] != ASSINATURA {
            return Err(invalido);
        }
        let versao = u16::from_le_bytes([arquivo[8], arquivo[9]]);
        if versao > VERSAO_2 {
            return Err(ErroDeSegredo::FormatoNaoSuportado { versao });
        }
        if versao < VERSAO_1
            || arquivo.len() < TAMANHO_DO_CABECALHO + TAMANHO_DA_ETIQUETA
            || arquivo[10] != KDF_ARGON2ID
            || arquivo[TAMANHO_DO_CABECALHO - TAMANHO_DO_NONCE - 1] != CIFRA_XCHACHA20POLY1305
        {
            return Err(invalido);
        }
        let parametros = ParametrosDeDerivacao {
            memoria_kib: u32_em(arquivo, 11),
            iteracoes: u32_em(arquivo, 15),
            paralelismo: u32_em(arquivo, 19),
        };
        if !parametros.legiveis() {
            return Err(invalido);
        }
        let mut sal = [0u8; TAMANHO_DO_SAL];
        sal.copy_from_slice(&arquivo[23..23 + TAMANHO_DO_SAL]);
        let mut verificador = [0u8; TAMANHO_DO_VERIFICADOR];
        verificador.copy_from_slice(&arquivo[39..39 + TAMANHO_DO_VERIFICADOR]);
        let mut nonce = [0u8; TAMANHO_DO_NONCE];
        nonce.copy_from_slice(
            &arquivo[TAMANHO_DO_CABECALHO - TAMANHO_DO_NONCE..TAMANHO_DO_CABECALHO],
        );
        Ok(Self {
            versao,
            derivacao: Derivacao {
                parametros,
                sal,
                verificador,
            },
            nonce,
            aad: &arquivo[..TAMANHO_DO_CABECALHO],
            corpo: &arquivo[TAMANHO_DO_CABECALHO..],
        })
    }

    /// Deriva a chave, confere o verificador e decifra. Senha errada e arquivo adulterado
    /// caem no mesmo erro: a decifração é tentada mesmo com o verificador errado, e só o
    /// resultado conjunto decide.
    pub(crate) fn destrancar(&self, senha: &[u8]) -> Result<Aberto, ErroDeSegredo> {
        let d = &self.derivacao;
        let (verificador, chave) = argon2id(&d.parametros, senha, &d.sal)?;
        let verificador_confere = iguais(verificador.as_ref(), &d.verificador);

        let (corpo, etiqueta) = self.corpo.split_at(self.corpo.len() - TAMANHO_DA_ETIQUETA);
        let mut buffer = Zeroizing::new(corpo.to_vec());
        let etiqueta = chacha20poly1305::Tag::try_from(etiqueta)
            .map_err(|_| ErroDeSegredo::SenhaOuArquivoInvalido)?;
        let decifrou = cifra(&chave)?
            .decrypt_inout_detached(
                &XNonce::from(self.nonce),
                self.aad,
                buffer.as_mut_slice().into(),
                &etiqueta,
            )
            .is_ok();
        if !(verificador_confere && decifrou) {
            buffer.zeroize();
            return Err(ErroDeSegredo::SenhaOuArquivoInvalido);
        }
        Ok(Aberto {
            versao: self.versao,
            derivacao: d.clone(),
            chave,
            conteudo: buffer,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selar_e_destrancar_ida_e_volta() {
        let (derivacao, chave) = derivar_nova(
            ParametrosDeDerivacao::rapidos_para_testes(),
            b"senha-ficticia",
        )
        .unwrap();
        let arquivo = selar(&derivacao, &chave, b"conteudo ficticio").unwrap();
        assert_eq!(
            arquivo.len(),
            TAMANHO_DO_CABECALHO + 17 + TAMANHO_DA_ETIQUETA
        );
        let lido = ArquivoCifrado::ler(&arquivo).unwrap();
        let aberto = lido.destrancar(b"senha-ficticia").unwrap();
        assert_eq!(aberto.versao, VERSAO_2);
        assert_eq!(aberto.conteudo.as_slice(), b"conteudo ficticio");
        assert_eq!(aberto.derivacao, derivacao);
        assert!(matches!(
            lido.destrancar(b"outra-senha"),
            Err(ErroDeSegredo::SenhaOuArquivoInvalido)
        ));
    }

    #[test]
    fn cada_gravacao_usa_nonce_novo() {
        let (derivacao, chave) = derivar_nova(
            ParametrosDeDerivacao::rapidos_para_testes(),
            b"senha-ficticia",
        )
        .unwrap();
        let a = selar(&derivacao, &chave, b"mesmo conteudo").unwrap();
        let b = selar(&derivacao, &chave, b"mesmo conteudo").unwrap();
        assert_eq!(a[..72], b[..72], "mesma derivação");
        assert_ne!(a[72..96], b[72..96], "nonce novo");
        assert_ne!(a[96..], b[96..]);
    }
}
