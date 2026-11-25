//! A limpeza de segredo que o app injeta (`Verbos::limpar`) vale para todo erro e registro.

mod luau_comum;

use bancada_navegador::luau::{ErroDeRoteiro, ErroDeVerbo, Limites, executar};
use luau_comum::VerbosFalsos;

const SEGREDO: &str = "S3nha-Fict1cia";

fn com_segredo() -> VerbosFalsos {
    VerbosFalsos::novo().com_segredo(SEGREDO)
}

#[test]
fn registros_da_saida_passam_pela_limpeza() {
    let mut v = com_segredo();
    let saida = executar(
        &format!(r##"navegador.registrar("a senha é {SEGREDO}, ok?")"##),
        &mut v,
        &Limites::default(),
    )
    .expect("ok");
    assert_eq!(
        saida.registros,
        vec!["a senha é [segredo], ok?".to_string()]
    );
}

#[test]
fn erro_de_execucao_do_roteiro_passa_pela_limpeza() {
    let mut v = com_segredo();
    let erro = executar(
        &format!(r##"error("falhou com {SEGREDO}")"##),
        &mut v,
        &Limites::default(),
    )
    .expect_err("erro");
    let texto = erro.to_string();
    assert!(!texto.contains(SEGREDO), "{texto}");
    assert!(texto.contains("[segredo]"), "{texto}");
}

#[test]
fn erro_de_verbo_fatal_passa_pela_limpeza() {
    let mut v = com_segredo().falhando(
        "clicar",
        ErroDeVerbo::Bloqueado {
            motivo: format!("botão com {SEGREDO}"),
        },
    );
    let erro = executar(r##"navegador.clicar("#x")"##, &mut v, &Limites::default())
        .expect_err("bloqueado");
    let texto = erro.to_string();
    assert!(!texto.contains(SEGREDO), "{texto}");
    assert!(matches!(
        erro,
        ErroDeRoteiro::Verbo {
            erro: ErroDeVerbo::Bloqueado { .. },
            ..
        }
    ));
}

#[test]
fn erro_de_verbo_capturado_com_pcall_chega_limpo_ao_script() {
    let mut v = com_segredo().falhando("clicar", ErroDeVerbo::Falhou(format!("viu {SEGREDO}")));
    let saida = executar(
        r##"
        local ok, e = pcall(navegador.clicar, "#x")
        assert(not ok)
        navegador.registrar(tostring(e))
        "##,
        &mut v,
        &Limites::default(),
    )
    .expect("ok");
    assert!(
        !saida.registros[0].contains(SEGREDO),
        "{:?}",
        saida.registros
    );
    assert!(saida.registros[0].contains("[segredo]"));
}

#[test]
fn erro_de_sintaxe_passa_pela_limpeza() {
    let mut v = com_segredo();
    let erro = executar(
        &format!("local {SEGREDO} = = 1"),
        &mut v,
        &Limites::default(),
    )
    .expect_err("sintaxe");
    assert!(!erro.to_string().contains(SEGREDO), "{erro}");
}

#[test]
fn sem_limpar_implementado_o_texto_passa_como_esta() {
    // Documenta o padrão do trait: quem não sobrescreve `limpar` não tem limpeza do host.
    let mut v = VerbosFalsos::novo();
    let saida = executar(
        r##"navegador.registrar("texto qualquer")"##,
        &mut v,
        &Limites::default(),
    )
    .expect("ok");
    assert_eq!(saida.registros, vec!["texto qualquer".to_string()]);
}
