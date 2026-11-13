//! Demonstração da casca com dados fictícios e genéricos:
//! `cargo run -p bancada-casca --example demonstracao`.
//!
//! Experimente: Alt (KeyTips), Ctrl+K (Paleta), Ctrl+F1 (recolher o Ribbon), Ctrl+/ (Mapa de
//! atalhos), Alt+←/→ e os botões laterais do mouse (histórico), Ctrl+clique num nome (painel
//! flutuante) e "Destacar em janela" no cabeçalho do painel flutuante.

#[path = "../tests/comum/app_exemplo.rs"]
mod app_exemplo;

use app_exemplo::AppExemplo;
use bancada_casca::Casca;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let casca = Casca::nova(AppExemplo::novo(), AppExemplo::config())?;
    casca.executar()?;
    Ok(())
}
