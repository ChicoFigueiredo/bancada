//! A biblioteca `string` do roteiro: padrões que travariam o VM são recusados na hora.

mod luau_comum;

use std::time::{Duration, Instant};

use bancada_navegador::luau::{ErroDeRoteiro, Limites, Saida, executar};
use luau_comum::VerbosFalsos;

fn rodar(fonte: &str) -> Result<Saida, ErroDeRoteiro> {
    let mut v = VerbosFalsos::novo();
    executar(fonte, &mut v, &Limites::default())
}

/// Roda e exige erro de execução rápido (< 1 s) com a mensagem esperada.
fn recusa_rapido(fonte: &str, trecho: &str) {
    let inicio = Instant::now();
    let erro = rodar(fonte).expect_err("deveria recusar");
    let gasto = inicio.elapsed();
    assert!(gasto < Duration::from_secs(1), "demorou {gasto:?}");
    let ErroDeRoteiro::Execucao { mensagem, linha } = &erro else {
        panic!("esperava erro de execução, veio {erro:?}");
    };
    assert!(mensagem.contains(trecho), "{mensagem}");
    assert!(linha.is_some(), "sem linha: {mensagem}");
}

#[test]
fn ataque_do_pareamento_com_dois_milhoes_de_parenteses_e_recusado() {
    recusa_rapido(r##"string.rep("(", 200000):find("%b()")"##, "%b");
}

#[test]
fn ataque_da_retroreferencia_e_recusado() {
    recusa_rapido(r##"("a"):rep(200000):find("(.-)%1b")"##, "retroreferência");
}

#[test]
fn os_ataques_com_milhoes_de_caracteres_nem_chegam_a_ser_montados() {
    // O texto original do ataque (3 milhões de "a") já passa do teto de `string.rep`.
    recusa_rapido(r##"("a"):rep(3e6):find("(.-)%1b")"##, "string.rep");
    recusa_rapido(r##"string.rep("(", 2e6):find("%b()")"##, "string.rep");
}

#[test]
fn ataque_com_vinte_megabytes_montado_por_concatenacao_e_recusado_rapido() {
    // 16 MB cabem no teto de memória (32 MB), mas não no de busca.
    for busca in [
        r##"s:find("(.-)%1b")"##,
        r##"s:find("%b()")"##,
        r##"s:find("a*b")"##,
    ] {
        let fonte = format!(
            r##"
            local s = ("a"):rep(1024 * 1024)
            for _ = 1, 4 do s = s .. s end
            {busca}
            "##
        );
        recusa_rapido(&fonte, "grande demais");
    }
}

#[test]
fn todas_as_funcoes_recusam_percent_b_e_retroreferencia() {
    for chamada in [
        r##"("x"):find("%b()")"##,
        r##"("x"):match("%b[]")"##,
        r##"("x"):gmatch("%b{}")"##,
        r##"("x"):gsub("%b()", "")"##,
        r##"string.find("x", "(.)%1")"##,
        r##"string.match("x", "(.)%9")"##,
        r##"string.gsub("x", "(.)%2", "")"##,
        r##"string.gmatch("x", "(.)%1")"##,
    ] {
        recusa_rapido(chamada, "não é permitid");
    }
}

#[test]
fn percent_percent_antes_de_b_e_literal_e_passa() {
    let saida = rodar(
        r##"
        assert(("100%b"):find("%%b") == 4)
        assert(("a%1"):match("%%1") == "%1")
        "##,
    );
    assert!(saida.is_ok(), "{saida:?}");
}

#[test]
fn subject_maior_que_256_kib_e_recusado_nas_quatro_funcoes() {
    for chamada in [
        r##"big:find("a")"##,
        r##"big:match("a")"##,
        r##"big:gmatch("a")"##,
        r##"big:gsub("a", "b")"##,
        r##"big:find("a", 1, true)"##,
    ] {
        let fonte = format!(r##"local big = ("a"):rep(256 * 1024 + 1) {chamada}"##);
        recusa_rapido(&fonte, "grande demais");
    }
}

#[test]
fn subject_no_limite_exato_passa() {
    let saida = rodar(
        r##"
        local s = ("a"):rep(256 * 1024)
        assert(s:find("a$") == 256 * 1024)
        "##,
    );
    assert!(saida.is_ok(), "{saida:?}");
}

#[test]
fn padrao_enorme_e_recusado() {
    recusa_rapido(
        r##"("a"):rep(5000):find(("a"):rep(2000), 1, true)"##,
        "padrão é grande demais",
    );
}

#[test]
fn busca_literal_continua_permitida_e_rapida() {
    let inicio = Instant::now();
    let saida = rodar(
        r##"
        local s = ("a"):rep(200 * 1024) .. "(x)%1b"
        local i, j = s:find("(x)%1b", 1, true)
        assert(i == 200 * 1024 + 1 and j == 200 * 1024 + 6)
        assert(s:find("%b()", 1, true) == nil)
        "##,
    );
    assert!(saida.is_ok(), "{saida:?}");
    assert!(inicio.elapsed() < Duration::from_secs(1));
}

#[test]
fn padroes_legitimos_seguem_funcionando() {
    let saida = rodar(
        r##"
        assert(("Total: 1234 reais"):match("%d+") == "1234")
        assert(("   oi mundo  "):match("^%s*(.-)%s*$") == "oi mundo")
        local a, b = ("chave=valor"):match("^(%w+)=(%w+)$")
        assert(a == "chave" and b == "valor")
        assert(("a-b-c"):gsub("-", "+") == "a+b+c")
        assert(("R$ 12,50"):gsub("%D", "") == "1250")
        -- gsub com tabela e com função
        assert(("$nome e $outro"):gsub("%$(%w+)", { nome = "Fulano", outro = "Beltrano" }) == "Fulano e Beltrano")
        assert(("1 2 3"):gsub("%d", function(d) return tonumber(d) * 2 end) == "2 4 6")
        local soma = 0
        for n in ("10,20,30"):gmatch("%d+") do soma = soma + tonumber(n) end
        assert(soma == 60)
        assert(("THE (quick) fox"):find("%((%a+)%)") == 5)
        assert(("x"):rep(3) == "xxx")
        assert(("ab"):rep(0) == "")
        assert(string.format("%d", 3) == "3")
        assert(("%f"):len() == 2)
        assert(("THE END"):find("%f[%a]%a+%f[%A]") == 1)
        "##,
    );
    assert!(saida.is_ok(), "{saida:?}");
}

#[test]
fn string_rep_acima_de_1_mib_e_recusado() {
    recusa_rapido(r##"string.rep("x", 1e9)"##, "string.rep");
    recusa_rapido(r##"string.rep("x", 1024 * 1024 + 1)"##, "string.rep");
    recusa_rapido(r##"string.rep(("x"):rep(1024), 1025)"##, "string.rep");
    recusa_rapido(r##"string.rep("x", math.huge)"##, "string.rep");
}

#[test]
fn string_rep_no_limite_passa() {
    let saida = rodar(r##"assert(#string.rep("x", 1024 * 1024) == 1024 * 1024)"##);
    assert!(saida.is_ok(), "{saida:?}");
}

#[test]
fn erros_de_tipo_das_funcoes_originais_continuam_claros() {
    let erro = rodar(r##"string.find(nil, "a")"##).expect_err("tipo");
    assert!(matches!(erro, ErroDeRoteiro::Execucao { .. }), "{erro:?}");
}

#[test]
fn padroes_de_backtracking_longos_nao_travam_o_roteiro() {
    // Cadeia de `*` sobre texto longo: polinomial no tamanho do texto.
    let inicio = Instant::now();
    let r = rodar(
        r##"
        local s = ("a"):rep(256 * 1024)
        s:find("a*a*a*a*a*a*b")
        "##,
    );
    let gasto = inicio.elapsed();
    // O Luau consulta o interrupt dentro do casamento: o teto de instruções corta.
    assert!(
        matches!(r, Ok(_) | Err(ErroDeRoteiro::LimiteDeInstrucoes(_))),
        "{r:?}"
    );
    assert!(gasto < Duration::from_secs(2), "demorou {gasto:?}");
}
