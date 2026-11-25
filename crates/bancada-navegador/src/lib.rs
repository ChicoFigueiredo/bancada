//! Motor de navegador da bancada (ADR-022 do SBF). Esqueleto: os módulos abaixo são
//! preenchidos por frentes separadas.
//!
//! - `cdp`, `chrome`, `navegador`, `pagina`, `politica`, `falso`, `site_falso`: o navegador
//!   (extraído do `sbf-roteiros`); `site_falso_conexoes`: o painel de conexões fictício
//!   (Meu Pluggy) para o roteiro de atualizar conexões.
//! - `luau`: o roteiro escrito, com verbos de alto nível sobre o trait `luau::Verbos`.

pub mod cdp;
pub mod chrome;
pub mod erro;
#[cfg(any(test, feature = "testes"))]
pub mod falso;
pub mod luau;
pub mod navegador;
pub mod pagina;
pub mod politica;
pub mod site_falso;
pub mod site_falso_conexoes;
pub mod url;

pub use erro::{ErroDoNavegador, Resultado};
pub use navegador::Navegador;
pub use pagina::{MotivoDePausa, Pagina};
pub use politica::{Bloqueio, Candidato, Politica};
