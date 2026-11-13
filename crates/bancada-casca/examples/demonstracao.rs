//! Demonstração da casca com dados fictícios e genéricos:
//! `cargo run -p bancada-casca --example demonstracao`.
//!
//! Experimente: Alt (KeyTips), Ctrl+K (Paleta), Ctrl+F1 (recolher o Ribbon), Ctrl+/ (Mapa de
//! atalhos), Alt+←/→ e os botões laterais do mouse (histórico), Ctrl+clique num nome (painel
//! flutuante) e "Destacar em janela" no cabeçalho do painel flutuante.
//!
//! Para conferência visual sem mexer na janela: `BANCADA_CAPTURA=/caminho/tela.ppm` grava uma
//! captura (PPM) depois de alguns quadros e fecha. `BANCADA_CENA` escolhe o que mostrar:
//! `paleta`, `keytips`, `menu`, `destacado`, `simples`, `recolhido`, `bastidores`, `flutuante`, `escuro`.

#[path = "../tests/comum/app_exemplo.rs"]
mod app_exemplo;

use app_exemplo::AppExemplo;
use bancada_casca::{Casca, Local, ModoRibbon, Pedido, eframe, egui, pedir};

struct Demonstracao {
    casca: Casca<AppExemplo>,
    captura: Option<String>,
    cena: String,
    quadros: u32,
}

impl Demonstracao {
    fn preparar_cena(&mut self, ctx: &egui::Context) {
        let e = &mut self.casca.estado;
        match self.cena.as_str() {
            "paleta" => {
                e.paleta.abrir();
                e.paleta.busca = "pro".into();
            }
            "keytips" => self.casca.mostrar_keytips(),
            "menu" => pedir(ctx, Pedido::Executar("filtro.estado".into())),
            "destacado" => {
                let id = e.flutuantes.abrir("contato:ana".into());
                e.flutuantes.destacar(id, true);
            }
            "simples" => e.ribbon.definir_modo(ModoRibbon::Simples),
            "recolhido" => {
                e.ribbon.definir_modo(ModoRibbon::Recolhido);
                e.ribbon.aba_ativa = Some("triagem".into());
                e.ribbon.corpo_aberto = true;
            }
            "bastidores" => {
                e.ribbon.bastidores_abertos = true;
                e.secao_bastidores = Some("opcoes".into());
            }
            "flutuante" => {
                pedir(
                    ctx,
                    Pedido::Abrir {
                        rota: "projeto:jardim".into(),
                        local: Local::Centro,
                    },
                );
                pedir(ctx, Pedido::AbrirFlutuante("contato:ana".into()));
                self.casca.app.selecionado = Some("jardim".into());
            }
            "escuro" => {
                self.casca.definir_aparencia(Some("noite"), None, None);
                pedir(
                    ctx,
                    Pedido::Abrir {
                        rota: "projetos".into(),
                        local: Local::Centro,
                    },
                );
            }
            _ => {}
        }
    }
}

impl eframe::App for Demonstracao {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.quadros == 1 {
            self.preparar_cena(&ctx);
        }
        self.casca.desenhar(ui);
        self.quadros += 1;
        let Some(caminho) = &self.captura else {
            return;
        };
        if self.quadros == 12 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        let imagem = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = imagem {
            let mut ppm = format!("P6\n{} {}\n255\n", img.size[0], img.size[1]).into_bytes();
            for p in &img.pixels {
                ppm.extend_from_slice(&[p.r(), p.g(), p.b()]);
            }
            if let Err(erro) = std::fs::write(caminho, ppm) {
                eprintln!("não gravei a captura: {erro}");
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint();
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let casca = Casca::nova(AppExemplo::novo(), AppExemplo::config())?;
    let captura = std::env::var("BANCADA_CAPTURA").ok();
    let cena = std::env::var("BANCADA_CENA").unwrap_or_default();
    if captura.is_none() && cena.is_empty() {
        casca.executar()?;
        return Ok(());
    }
    let opcoes = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Demonstração da bancada",
        opcoes,
        Box::new(move |_cc| {
            Ok(Box::new(Demonstracao {
                casca,
                captura,
                cena,
                quadros: 0,
            }))
        }),
    )?;
    Ok(())
}
