//! Testes estáticos sobre o código-fonte da bancada: só a guarda lê um segredo em claro
//! (`Segredo::expor`), o chaveiro falso só entra em testes e o `Segredo` não deriva `Debug` nem
//! tem `Display`.

use std::fs;
use std::path::{Path, PathBuf};

/// Os únicos arquivos de produção que leem um segredo em claro.
const LEITORES: [&str; 4] = [
    "bancada-segredos/src/segredo.rs",
    "bancada-segredos/src/chaveiro.rs",
    "bancada-segredos/src/conteudo.rs",
    "bancada-segredos/src/guarda.rs",
];

fn crates_do_workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn fontes_rust(dir: &Path, saida: &mut Vec<PathBuf>) {
    for entrada in fs::read_dir(dir).expect("lê diretório") {
        let caminho = entrada.expect("entrada").path();
        if caminho.is_dir() {
            fontes_rust(&caminho, saida);
        } else if caminho.extension().is_some_and(|e| e == "rs") {
            saida.push(caminho);
        }
    }
}

/// Código de produção (`crates/*/src`) sem os módulos `#[cfg(test)]`.
fn fontes_de_producao() -> Vec<(String, String)> {
    let mut arquivos = Vec::new();
    for krate in fs::read_dir(crates_do_workspace()).expect("crates") {
        let src = krate.expect("crate").path().join("src");
        if src.is_dir() {
            fontes_rust(&src, &mut arquivos);
        }
    }
    let mut saida: Vec<(String, String)> = arquivos
        .into_iter()
        .map(|p| {
            let relativo = p
                .strip_prefix(crates_do_workspace())
                .expect("dentro de crates")
                .to_string_lossy()
                .replace('\\', "/");
            let texto = fs::read_to_string(&p).expect("lê fonte");
            let codigo = texto.split("#[cfg(test)]").next().unwrap_or("").to_owned();
            (relativo, codigo)
        })
        .collect();
    saida.sort();
    saida
}

#[test]
fn so_a_guarda_le_um_segredo_em_claro() {
    for (arquivo, codigo) in fontes_de_producao() {
        if codigo.contains(".expor()") || codigo.contains("fn expor") {
            assert!(
                LEITORES.contains(&arquivo.as_str()),
                "{arquivo} lê segredo em claro fora da guarda"
            );
        }
    }
}

#[test]
fn o_codigo_de_producao_nao_escreve_em_log_nem_na_saida() {
    for (arquivo, codigo) in fontes_de_producao() {
        if !arquivo.starts_with("bancada-segredos/") {
            continue;
        }
        for proibido in [
            "println!",
            "eprintln!",
            "print!(",
            "eprint!(",
            "dbg!",
            "log::",
            "tracing::",
        ] {
            assert!(!codigo.contains(proibido), "{proibido} em {arquivo}");
        }
    }
}

#[test]
fn o_chaveiro_falso_so_e_ligado_em_testes() {
    for krate in fs::read_dir(crates_do_workspace()).expect("crates") {
        let dir = krate.expect("crate").path();
        let Ok(texto) = fs::read_to_string(dir.join("Cargo.toml")) else {
            continue;
        };
        let mut secao = String::new();
        for linha in texto.lines() {
            let linha = linha.trim();
            if linha.starts_with('[') {
                secao = linha.to_owned();
                continue;
            }
            if linha.starts_with("bancada-segredos") && linha.contains("testes") {
                assert!(
                    secao.contains("dev-dependencies"),
                    "{} liga o chaveiro falso fora de [dev-dependencies] ({secao})",
                    dir.display()
                );
            }
        }
    }
    let lib =
        fs::read_to_string(crates_do_workspace().join("bancada-segredos/src/lib.rs")).expect("lib");
    assert!(
        lib.contains("#[cfg(feature = \"testes\")]\npub mod testes;"),
        "o chaveiro falso precisa ficar atrás da feature `testes`"
    );
}

#[test]
fn o_segredo_esconde_o_debug_e_nao_tem_display() {
    let fonte = fs::read_to_string(crates_do_workspace().join("bancada-segredos/src/segredo.rs"))
        .expect("segredo.rs");
    assert!(!fonte.contains("Display for Segredo"));
    assert!(fonte.contains("impl fmt::Debug for Segredo"));
    let em = fonte.find("pub struct Segredo").expect("tipo");
    let atributos = &fonte[fonte[..em].rfind("\n\n").unwrap_or(0)..em];
    assert!(
        !atributos
            .lines()
            .any(|l| l.trim_start().starts_with("#[derive") && l.contains("Debug")),
        "Segredo não pode derivar Debug"
    );
    assert!(
        !atributos
            .lines()
            .any(|l| l.trim_start().starts_with("#[derive") && l.contains("Serialize")),
        "Segredo não pode derivar Serialize"
    );
}
