//! Guarda de código: o valor de um `Segredo` só é exposto onde precisa.
//!
//! `Segredo::expor` pode aparecer em `chrome.rs` (entrega ao CDP) e em `falso.rs` (navegador
//! de teste). Nenhum outro arquivo de produção da raiz do crate chama (o host Luau recebe o
//! rótulo do chaveiro e passa o segredo adiante pelo trait `Navegador`, sem abri-lo).
//! Vinha do `sbf-roteiros` junto com o código.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

const PERMITIDOS: &[&str] = &["chrome.rs", "falso.rs"];

fn fontes() -> Vec<(String, String)> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    std::fs::read_dir(src)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "rs"))
        .map(|p| {
            let nome = p.file_name().unwrap().to_str().unwrap().to_owned();
            (nome, std::fs::read_to_string(&p).unwrap())
        })
        .collect()
}

#[test]
fn so_quem_precisa_expoe_o_segredo() {
    for (nome, texto) in fontes() {
        if texto.contains(".expor()") {
            assert!(
                PERMITIDOS.contains(&nome.as_str()),
                "{nome} chama Segredo::expor"
            );
        }
    }
}

#[test]
fn nenhum_log_nem_print_no_crate() {
    for (nome, texto) in fontes() {
        // os testes unitários do próprio arquivo podem imprimir; a produção não
        let producao = texto.split("#[cfg(test)]").next().unwrap();
        for proibido in ["println!", "eprintln!", "dbg!(", "log::", "tracing::"] {
            assert!(!producao.contains(proibido), "{nome}: {proibido}");
        }
    }
}
