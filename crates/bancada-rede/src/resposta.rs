//! O resultado de um pedido: corpo, origem, `ETag` e frescor.

use crate::erro::ErroDeRede;
use crate::relogio::Instante;
use std::fmt;
use std::time::Duration;

/// De onde veio a resposta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frescor {
    /// Da rede, agora.
    Rede,
    /// Do cache do portão, confirmado agora pelo servidor (`304`).
    Revalidada,
    /// Do cache do portão, dentro da validade; a rede não foi tocada.
    Cache,
    /// O servidor confirmou (`304`) que a `ETag` de quem pediu ([`crate::Pedido::se_mudou_desde`])
    /// continua valendo. O corpo vem vazio: quem pediu já tem o conteúdo.
    NaoMudou,
    /// Do cache vencido, porque a rede falhou (`causa`): mostrar com a idade.
    Vencida { causa: ErroDeRede },
}

/// Resposta entregue a quem pediu.
#[derive(Clone, PartialEq, Eq)]
pub struct Resposta {
    pub status: u16,
    pub tipo: Option<String>,
    pub corpo: Vec<u8>,
    pub etag: Option<String>,
    /// URL oficial consultada (sem consulta secreta, sem a rota dos testes).
    pub origem: String,
    /// Última vez que o servidor confirmou o conteúdo (resposta ou `304`).
    pub consultada_em: Instante,
    /// Idade do conteúdo: agora menos `consultada_em`.
    pub idade: Duration,
    pub frescor: Frescor,
}

impl Resposta {
    /// O corpo como texto, se for UTF-8.
    pub fn texto(&self) -> Option<&str> {
        std::str::from_utf8(&self.corpo).ok()
    }
}

/// O `Debug` mostra o tamanho do corpo, não o conteúdo (pode ser dado financeiro ou token).
impl fmt::Debug for Resposta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Resposta")
            .field("status", &self.status)
            .field("tipo", &self.tipo)
            .field("corpo", &format_args!("{} bytes", self.corpo.len()))
            .field("etag", &self.etag)
            .field("origem", &self.origem)
            .field("consultada_em", &self.consultada_em)
            .field("idade", &self.idade)
            .field("frescor", &self.frescor)
            .finish()
    }
}
