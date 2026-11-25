//! O verbo de leitura do trait `Navegador`: texto visível, nunca valor de campo.
#![cfg(feature = "testes")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use bancada_navegador::Navegador;
use bancada_navegador::falso::{Elemento, NavegadorFalso};
use bancada_navegador::pagina::{Campo, Elemento as ElementoDePagina, Formulario, Pagina};

fn falso() -> NavegadorFalso {
    let mut n = NavegadorFalso::default();
    n.pagina_nomeada(
        "p",
        Pagina {
            elementos: vec![ElementoDePagina {
                seletor: "#titulo".into(),
                texto: "Demonstrativo de novembro".into(),
                ..ElementoDePagina::default()
            }],
            formularios: vec![Formulario {
                seletor: "#f".into(),
                campos: vec![Campo {
                    seletor: "#senha".into(),
                    tipo: "password".into(),
                    ..Campo::default()
                }],
                ..Formulario::default()
            }],
            ..Pagina::default()
        },
    );
    n.atual = "p".into();
    let mut campo = Elemento::com_texto("");
    campo.candidato.eh_campo = true;
    n.elementos.insert("#senha".into(), campo);
    n
}

#[test]
fn texto_devolve_o_texto_visivel_do_elemento() {
    let mut n = falso();
    assert_eq!(n.texto("#titulo").unwrap(), "Demonstrativo de novembro");
}

#[test]
fn texto_recusa_campo_de_formulario_mesmo_com_valor_digitado() {
    use bancada_segredos::Segredo;
    let mut n = falso();
    n.preencher_segredo("#senha", &Segredo::novo("SenhaFicticia#1"))
        .unwrap();
    let e = n.texto("#senha").unwrap_err().to_string();
    assert!(e.contains("campo de formulário"), "{e}");
    assert!(!e.contains("SenhaFicticia"), "{e}");
}

#[test]
fn texto_de_elemento_que_nao_existe_e_erro() {
    let mut n = falso();
    assert!(n.texto("#nada").is_err());
}
