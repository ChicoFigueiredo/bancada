//! Os parâmetros do roteiro (tabela global `parametros`), `util.meses` e o verbo
//! `navegador.entrada_recusada`.

mod luau_comum;

use bancada_navegador::luau::{ErroDeRoteiro, Limites, Parametros, Saida, executar, executar_com};
use luau_comum::VerbosFalsos;

fn rodar(fonte: &str, v: &mut VerbosFalsos, p: &Parametros) -> Result<Saida, ErroDeRoteiro> {
    executar_com(fonte, v, &Limites::default(), p)
}

fn parametros_de_mes() -> Parametros {
    let mut p = Parametros::novo();
    p.inserir_texto("de", "2026-10");
    p.inserir_texto("ate", "2026-11");
    p.inserir_booleano("forcar", false);
    p
}

#[test]
fn o_roteiro_le_os_parametros_ja_resolvidos() {
    let mut v = VerbosFalsos::novo();
    let saida = rodar(
        r##"
        assert(parametros.de == "2026-10", "de")
        assert(parametros.ate == "2026-11", "ate")
        assert(parametros.forcar == false, "forcar")
        assert(type(parametros.forcar) == "boolean")
        navegador.registrar(parametros.de .. ".." .. parametros.ate)
        "##,
        &mut v,
        &parametros_de_mes(),
    )
    .expect("lê os parâmetros");
    assert_eq!(saida.registros, vec!["2026-10..2026-11"]);
}

#[test]
fn sem_parametros_a_tabela_existe_e_vazia() {
    let mut v = VerbosFalsos::novo();
    rodar(
        r##"
        assert(type(parametros) == "table")
        assert(next(parametros) == nil)
        assert(parametros.de == nil)
        "##,
        &mut v,
        &Parametros::novo(),
    )
    .expect("tabela vazia");
}

#[test]
fn o_executar_antigo_continua_com_a_tabela_vazia() {
    let mut v = VerbosFalsos::novo();
    executar(
        r##"assert(next(parametros) == nil)"##,
        &mut v,
        &Limites::default(),
    )
    .expect("atalho");
}

#[test]
fn a_tabela_de_parametros_e_somente_leitura() {
    let tentativas = [
        r##"parametros.de = "2020-01""##,
        r##"parametros.novo = "x""##,
        r##"parametros.forcar = true"##,
        r##"rawset(parametros, "de", "2020-01")"##,
        r##"table.insert(parametros, "x")"##,
        r##"setmetatable(parametros, {})"##,
    ];
    for fonte in tentativas {
        let mut v = VerbosFalsos::novo();
        let erro = rodar(fonte, &mut v, &parametros_de_mes()).expect_err(fonte);
        assert!(
            matches!(erro, ErroDeRoteiro::Execucao { .. }),
            "{fonte}: {erro:?}"
        );
    }
}

#[test]
fn trocar_o_global_parametros_nao_vaza_para_o_host() {
    // O script pode sombrear o nome no seu próprio ambiente, mas isso não muda nada fora dele.
    let mut v = VerbosFalsos::novo();
    rodar(
        r##"parametros = { de = "1999-01" }"##,
        &mut v,
        &parametros_de_mes(),
    )
    .ok();
    let mut v2 = VerbosFalsos::novo();
    rodar(
        r##"assert(parametros.de == "2026-10")"##,
        &mut v2,
        &parametros_de_mes(),
    )
    .expect("cada execução tem o seu VM");
}

#[test]
fn util_meses_lista_em_ordem() {
    let mut v = VerbosFalsos::novo();
    let saida = rodar(
        r##"
        local lista = util.meses("2026-11", "2027-02")
        navegador.registrar(table.concat(lista, ","))
        local um = util.meses("2026-11", "2026-11")
        navegador.registrar(#um .. ":" .. um[1])
        "##,
        &mut v,
        &Parametros::novo(),
    )
    .expect("util.meses");
    assert_eq!(
        saida.registros,
        vec!["2026-11,2026-12,2027-01,2027-02", "1:2026-11"]
    );
}

#[test]
fn util_meses_aceita_os_parametros() {
    let mut v = VerbosFalsos::novo();
    let saida = rodar(
        r##"navegador.registrar(table.concat(util.meses(parametros.de, parametros.ate), " "))"##,
        &mut v,
        &parametros_de_mes(),
    )
    .expect("meses dos parâmetros");
    assert_eq!(saida.registros, vec!["2026-10 2026-11"]);
}

#[test]
fn util_meses_recusa_de_maior_que_ate() {
    let mut v = VerbosFalsos::novo();
    let erro = rodar(
        r##"util.meses("2026-12", "2026-11")"##,
        &mut v,
        &Parametros::novo(),
    )
    .expect_err("de > ate");
    assert!(
        matches!(&erro, ErroDeRoteiro::Execucao { mensagem, .. } if mensagem.contains("util.meses")),
        "{erro:?}"
    );
}

#[test]
fn util_meses_tem_teto_de_60_meses() {
    let mut v = VerbosFalsos::novo();
    // 60 meses exatos passam; 61 não.
    rodar(
        r##"assert(#util.meses("2022-01", "2026-12") == 60)"##,
        &mut v,
        &Parametros::novo(),
    )
    .expect("60 meses");
    let erro = rodar(
        r##"util.meses("2022-01", "2027-01")"##,
        &mut v,
        &Parametros::novo(),
    )
    .expect_err("61 meses");
    assert!(matches!(erro, ErroDeRoteiro::Execucao { .. }), "{erro:?}");
}

#[test]
fn util_meses_recusa_formato_e_tipo_errados() {
    for fonte in [
        r##"util.meses("2026-13", "2026-12")"##,
        r##"util.meses("2026-00", "2026-12")"##,
        r##"util.meses("2026-1", "2026-12")"##,
        r##"util.meses("26-01", "2026-12")"##,
        r##"util.meses("2026-01-01", "2026-12")"##,
        r##"util.meses("abc", "2026-12")"##,
        r##"util.meses("2026-01")"##,
        r##"util.meses(1, 2)"##,
        r##"util.meses("2026-01", "2026-02", "2026-03")"##,
    ] {
        let mut v = VerbosFalsos::novo();
        let erro = rodar(fonte, &mut v, &Parametros::novo()).expect_err(fonte);
        assert!(
            matches!(erro, ErroDeRoteiro::Execucao { .. }),
            "{fonte}: {erro:?}"
        );
        assert!(v.chamadas.is_empty(), "{fonte}: não chama verbo do app");
    }
}

#[test]
fn entrada_recusada_encerra_com_erro_proprio() {
    let mut v = VerbosFalsos::novo();
    let erro = rodar(
        r##"
        navegador.registrar("antes")
        navegador.entrada_recusada("o portal pediu para atualizar o cadastro")
        navegador.registrar("depois")
        "##,
        &mut v,
        &Parametros::novo(),
    )
    .expect_err("entrada recusada");
    assert_eq!(
        erro,
        ErroDeRoteiro::EntradaRecusada("o portal pediu para atualizar o cadastro".into())
    );
    assert_eq!(v.chamadas.len(), 1, "o registrar de depois não roda");
}

#[test]
fn entrada_recusada_nao_se_captura_com_pcall() {
    let mut v = VerbosFalsos::novo();
    let erro = rodar(
        r##"
        local ok = pcall(function() navegador.entrada_recusada("senha expirada") end)
        navegador.registrar("depois do pcall")
        "##,
        &mut v,
        &Parametros::novo(),
    )
    .expect_err("fatal mesmo com pcall");
    assert_eq!(
        erro,
        ErroDeRoteiro::EntradaRecusada("senha expirada".into())
    );
    assert!(v.chamadas.is_empty());
}

#[test]
fn entrada_recusada_limpa_segredo_e_limita_o_tamanho() {
    let mut v = VerbosFalsos::novo().com_segredo("hunter2");
    let erro = rodar(
        r##"navegador.entrada_recusada("senha hunter2 recusada")"##,
        &mut v,
        &Parametros::novo(),
    )
    .expect_err("recusada");
    let ErroDeRoteiro::EntradaRecusada(m) = &erro else {
        panic!("{erro:?}");
    };
    assert!(!m.contains("hunter2"), "{m}");
    assert!(m.contains("[segredo]"), "{m}");
    assert!(!format!("{erro:?}{erro}").contains("hunter2"));

    let mut v = VerbosFalsos::novo();
    let erro = rodar(
        r##"navegador.entrada_recusada(string.rep("a", 500))"##,
        &mut v,
        &Parametros::novo(),
    )
    .expect_err("recusada");
    let ErroDeRoteiro::EntradaRecusada(m) = erro else {
        panic!("tipo");
    };
    assert_eq!(m.chars().count(), 200);
}

#[test]
fn entrada_recusada_exige_um_texto() {
    for fonte in [
        r##"navegador.entrada_recusada()"##,
        r##"navegador.entrada_recusada(1)"##,
        r##"navegador.entrada_recusada("a", "b")"##,
    ] {
        let mut v = VerbosFalsos::novo();
        let erro = rodar(fonte, &mut v, &Parametros::novo()).expect_err(fonte);
        assert!(
            matches!(erro, ErroDeRoteiro::Execucao { .. }),
            "{fonte}: {erro:?}"
        );
    }
}
