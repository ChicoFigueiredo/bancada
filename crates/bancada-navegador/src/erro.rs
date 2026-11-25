//! O erro do navegador. Nenhuma mensagem leva valor de campo, senha ou conteúdo de página.
//! O app converte este erro no seu (no SBF, `ErroDeRoteiro`).

/// Falha do navegador ou do CDP.
#[derive(Debug, thiserror::Error)]
pub enum ErroDoNavegador {
    #[error("disco: {0}")]
    Disco(#[from] std::io::Error),
    /// Falha do navegador ou do CDP (sem valores enviados).
    #[error("navegador: {0}")]
    Navegador(String),
}

pub type Resultado<T> = Result<T, ErroDoNavegador>;
