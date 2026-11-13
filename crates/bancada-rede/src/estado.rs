//! Estado da rede para a barra de status e o diagnóstico sanitizado das últimas operações.

use crate::erro::ErroDeRede;
use crate::pedido::Metodo;
use crate::relogio::Instante;
use crate::resposta::Frescor;

/// Situação de um host do catálogo agora.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SituacaoDoHost {
    /// Nenhum problema conhecido (ou ainda não houve pedido).
    Disponivel,
    /// O servidor pediu pausa: nada sai para ele antes de `ate`.
    Limitado { ate: Instante },
    /// A última tentativa falhou (rede, prazo ou erro do servidor) desde `desde`.
    Falhando { desde: Instante, erro: ErroDeRede },
}

/// Fotografia do estado da rede (barata: pode ser lida a cada quadro).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EstadoDaRede {
    /// Um por host do catálogo (`host[:porta]`), na ordem do catálogo.
    pub hosts: Vec<(String, SituacaoDoHost)>,
    /// Pedidos em andamento ou na fila.
    pub pedidos_ativos: usize,
    /// Última resposta que chegou de qualquer host.
    pub ultimo_contato: Option<Instante>,
}

impl EstadoDaRede {
    /// A situação de um host (`Disponivel` se ele não está no catálogo).
    pub fn situacao(&self, host: &str) -> &SituacaoDoHost {
        self.hosts
            .iter()
            .find(|(h, _)| h == host)
            .map_or(&SituacaoDoHost::Disponivel, |(_, s)| s)
    }

    /// O host está sem rede agora (a última tentativa não o alcançou).
    pub fn sem_rede(&self, host: &str) -> bool {
        matches!(self.situacao(host), SituacaoDoHost::Falhando { erro, .. } if erro.e_falta_de_rede())
    }
}

/// Registro sanitizado de uma operação: sem cabeçalhos, consulta, corpo ou segredo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Operacao {
    pub metodo: Metodo,
    pub host: String,
    /// Só o caminho, sem a consulta.
    pub caminho: String,
    pub inicio: Instante,
    pub fim: Instante,
    pub status_http: Option<u16>,
    pub resultado: Result<Frescor, ErroDeRede>,
}
