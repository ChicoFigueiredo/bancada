//! O exemplo mínimo do README (mantido igual a ele): `cargo run -p bancada-casca --example minimo`.

use bancada_casca::{
    Aba, Acao, AppCasca, Atalho, Casca, ConfigCasca, Elo, Grupo, GrupoNavegacao, ItemNavegacao,
    Local, Rota, egui, link_item,
};

struct MeuApp;

impl AppCasca for MeuApp {
    fn acoes(&self) -> Vec<Acao> {
        vec![
            Acao::nova("triagem.descartar", "Descartar")
                .icone("delete")
                .keytip("X")
                .atalho(Atalho::tecla(egui::Key::Delete)),
            Acao::nova("lote.enviar", "Enviar ao servidor")
                .desabilitada("Chega na fatia 3, com a caixa de entrada"),
        ]
    }
    fn abas(&self) -> Vec<Aba> {
        vec![
            Aba::nova("triagem", "Triagem", "T")
                .grupo(Grupo::novo("triagem", "Triagem", &["triagem.descartar"]))
                .grupo(Grupo::novo("lotes", "Lotes", &["lote.enviar"])),
        ]
    }
    fn navegacao(&self) -> Vec<GrupoNavegacao> {
        vec![GrupoNavegacao::novo(
            "Inventário",
            vec![
                ItemNavegacao::novo("fornecedores", "Fornecedores", "store").contagem(12),
                ItemNavegacao::novo("hoje", "Hoje", "today")
                    .indisponivel("fatia 2", "Chega com o livro-caixa"),
            ],
        )]
    }
    fn rota_inicial(&self) -> Rota {
        Rota::nova("fornecedores")
    }
    fn trilha(&self, rota: &Rota) -> Vec<Elo> {
        match rota.como_texto() {
            "fornecedor:energia" => vec![
                Elo::link("Fornecedores", "fornecedores"),
                Elo::texto("Energia Exemplo"),
            ],
            _ => vec![Elo::texto("Fornecedores")],
        }
    }
    fn desenhar_pagina(&mut self, ui: &mut egui::Ui, rota: &Rota, _local: Local) {
        if rota.como_texto() == "fornecedores" {
            link_item(ui, "Energia Exemplo", "fornecedor:energia");
        }
    }
    fn ao_executar(&mut self, _ctx: &egui::Context, id: &str) {
        // grava a decisão etc.
        let _ = id;
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = ConfigCasca::nova(
        "Seu Bolso Feliz",
        include_str!("../dados-de-exemplo/tokens.json"),
        include_str!("../dados-de-exemplo/temas.json"),
    );
    config.tema_inicial = "salvia".into();
    Casca::nova(MeuApp, config)?.executar()?;
    Ok(())
}
