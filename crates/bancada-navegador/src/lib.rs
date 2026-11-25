//! Motor de navegador da bancada (ADR-022 do SBF). Esqueleto: os módulos abaixo são
//! preenchidos por frentes separadas.
//!
//! - `cdp`, `chrome`, `navegador`, `pagina`, `politica`, `falso`, `site_falso`: o navegador
//!   (extraído do `sbf-roteiros`).
//! - `luau`: o roteiro escrito, com verbos de alto nível sobre o trait `luau::Verbos`.

pub mod cdp;
pub mod chrome;
pub mod falso;
pub mod luau;
pub mod navegador;
pub mod pagina;
pub mod politica;
pub mod site_falso;
