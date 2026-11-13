//! Erros do portão. Nenhuma variante carrega cabeçalho de autorização, consulta ou corpo de
//! pedido; o corpo de uma resposta de erro vem num [`CorpoDeErro`], cujo `Debug` mostra só o
//! tamanho.

use crate::relogio::Instante;
use std::fmt;

/// Tamanho máximo guardado do corpo de uma resposta de erro.
pub const LIMITE_DO_CORPO_DE_ERRO: usize = 64 * 1024;

/// O corpo de uma resposta de erro (por exemplo o JSON `{"codigo": "estacao_revogada"}`),
/// cortado em [`LIMITE_DO_CORPO_DE_ERRO`]. O `Debug` mostra só o tamanho: quem precisa do
/// conteúdo o lê por [`CorpoDeErro::bytes`] ou [`CorpoDeErro::texto`].
#[derive(Clone, PartialEq, Eq, Default)]
pub struct CorpoDeErro(Vec<u8>);

impl CorpoDeErro {
    pub fn novo(mut bytes: Vec<u8>) -> Self {
        bytes.truncate(LIMITE_DO_CORPO_DE_ERRO);
        Self(bytes)
    }

    pub fn bytes(&self) -> &[u8] {
        &self.0
    }

    /// O corpo como texto, se for UTF-8.
    pub fn texto(&self) -> Option<&str> {
        std::str::from_utf8(&self.0).ok()
    }

    pub fn esta_vazio(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for CorpoDeErro {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CorpoDeErro({} bytes)", self.0.len())
    }
}

/// Por que um pedido não deu certo.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErroDeRede {
    /// O endereço não é HTTPS ou não está no catálogo: recusado antes de abrir conexão.
    EnderecoForaDoCatalogo { endereco: String },
    /// Pedido mal montado (cabeçalho proibido, corpo em `GET`…).
    PedidoInvalido { motivo: String },
    /// O pedido pede autorização, mas o endereço do catálogo não a recebe.
    EnderecoSemAutorizacao { endereco: String },
    /// O pedido pede autorização e não há credencial para o endereço: nada saiu.
    SemCredencial { endereco: String },
    /// O servidor recusou a credencial (`401` ou `403`). A fonte da autorização já foi
    /// avisada; o corpo diz o motivo, se o servidor o deu.
    AutorizacaoRecusada { status: u16, corpo: CorpoDeErro },
    /// O servidor pediu pausa (`429`): nada sai para ele antes de `ate`.
    Limitado { ate: Instante },
    /// Sem rota até o servidor (rede fora, DNS, conexão recusada, TLS).
    SemRede { detalhe: &'static str },
    /// O prazo do pedido passou.
    PrazoEsgotado,
    /// A conexão caiu no meio da resposta.
    Interrompida { detalhe: &'static str },
    /// O servidor respondeu com um redirecionamento, que o portão nunca segue.
    RedirecionamentoRecusado { status: u16 },
    /// A resposta passou do limite de bytes.
    CorpoGrandeDemais { limite: usize },
    /// O servidor respondeu com um erro que não é de autorização nem de limite.
    Http { status: u16, corpo: CorpoDeErro },
    /// Quem pediu desistiu.
    Cancelado,
    /// Falha do cache local.
    Armazenamento { detalhe: String },
}

impl ErroDeRede {
    pub(crate) fn invalido(motivo: impl Into<String>) -> Self {
        Self::PedidoInvalido {
            motivo: motivo.into(),
        }
    }

    pub(crate) fn armazenamento(erro: impl fmt::Display) -> Self {
        Self::Armazenamento {
            detalhe: erro.to_string(),
        }
    }

    /// O status HTTP, quando o servidor chegou a responder.
    pub fn status_http(&self) -> Option<u16> {
        match self {
            Self::AutorizacaoRecusada { status, .. }
            | Self::RedirecionamentoRecusado { status }
            | Self::Http { status, .. } => Some(*status),
            Self::Limitado { .. } => Some(429),
            _ => None,
        }
    }

    /// O corpo de uma resposta de erro, se houve.
    pub fn corpo(&self) -> Option<&CorpoDeErro> {
        match self {
            Self::AutorizacaoRecusada { corpo, .. } | Self::Http { corpo, .. } => Some(corpo),
            _ => None,
        }
    }

    /// O servidor não foi alcançado (vale mostrar a cópia local e tentar depois).
    pub fn e_falta_de_rede(&self) -> bool {
        matches!(
            self,
            Self::SemRede { .. } | Self::PrazoEsgotado | Self::Interrompida { .. }
        )
    }
}

impl fmt::Display for ErroDeRede {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EnderecoForaDoCatalogo { endereco } => write!(
                f,
                "o endereço {endereco} não é HTTPS ou não está no catálogo; nada foi enviado"
            ),
            Self::PedidoInvalido { motivo } => write!(f, "pedido inválido: {motivo}"),
            Self::EnderecoSemAutorizacao { endereco } => write!(
                f,
                "o endereço {endereco} não recebe credencial; nada foi enviado"
            ),
            Self::SemCredencial { endereco } => {
                write!(f, "não há credencial para {endereco}; nada foi enviado")
            }
            Self::AutorizacaoRecusada { status, .. } => {
                write!(f, "o servidor recusou a credencial (HTTP {status})")
            }
            Self::Limitado { ate } => write!(
                f,
                "o servidor pediu uma pausa até {} ms",
                ate.em_milissegundos()
            ),
            Self::SemRede { detalhe } => write!(f, "sem rede: {detalhe}"),
            Self::PrazoEsgotado => f.write_str("o servidor não respondeu a tempo"),
            Self::Interrompida { detalhe } => write!(f, "conexão interrompida: {detalhe}"),
            Self::RedirecionamentoRecusado { status } => write!(
                f,
                "o servidor respondeu com redirecionamento (HTTP {status}), que não é seguido"
            ),
            Self::CorpoGrandeDemais { limite } => {
                write!(f, "a resposta passou do limite de {limite} bytes")
            }
            Self::Http { status, .. } => write!(f, "o servidor respondeu HTTP {status}"),
            Self::Cancelado => f.write_str("pedido cancelado"),
            Self::Armazenamento { detalhe } => write!(f, "erro do cache da rede: {detalhe}"),
        }
    }
}

impl std::error::Error for ErroDeRede {}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn o_debug_do_corpo_mostra_so_o_tamanho() {
        let corpo = CorpoDeErro::novo(b"{\"codigo\":\"x\"}".to_vec());
        assert_eq!(format!("{corpo:?}"), "CorpoDeErro(14 bytes)");
        assert_eq!(corpo.texto(), Some("{\"codigo\":\"x\"}"));
    }

    #[test]
    fn o_corpo_e_cortado_no_limite() {
        let corpo = CorpoDeErro::novo(vec![b'a'; LIMITE_DO_CORPO_DE_ERRO + 10]);
        assert_eq!(corpo.bytes().len(), LIMITE_DO_CORPO_DE_ERRO);
    }
}
