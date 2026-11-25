//! O sandbox: nada de `os`, `io`, `require`, `loadstring`, `getfenv`, `debug`... e os verbos
//! de segredo só recebem rótulo.

mod luau_comum;

use bancada_navegador::luau::{ErroDeRoteiro, ErroDeVerbo, Limites, executar};
use luau_comum::{Chamada, VerbosFalsos};

fn rodar(
    fonte: &str,
    v: &mut VerbosFalsos,
) -> Result<bancada_navegador::luau::Saida, ErroDeRoteiro> {
    executar(fonte, v, &Limites::default())
}

#[test]
fn o_script_nao_alcanca_nada_perigoso() {
    let proibidos = [
        r##"os.execute("true")"##,
        r##"os.getenv("HOME")"##,
        r##"io.open("/etc/passwd")"##,
        r##"io.popen("ls")"##,
        r##"require("modulo")"##,
        r##"loadstring("return 1")()"##,
        r##"load("return 1")()"##,
        r##"dofile("/tmp/x.lua")"##,
        r##"getfenv(0)"##,
        r##"setfenv(1, {})"##,
        r##"debug.traceback()"##,
        r##"debug.getinfo(1)"##,
        r##"coroutine.wrap(function() end)()"##,
        r##"print("oi")"##,
        r##"collectgarbage("count")"##,
        r##"newproxy()"##,
        r##"_G.os.execute("true")"##,
        r##"package.loadlib("x", "y")"##,
        r##"string.dump(print)"##,
    ];
    for fonte in proibidos {
        let mut v = VerbosFalsos::novo();
        let erro = rodar(fonte, &mut v).expect_err(fonte);
        assert!(
            matches!(erro, ErroDeRoteiro::Execucao { .. }),
            "{fonte}: {erro:?}"
        );
        assert!(v.chamadas.is_empty(), "{fonte}");
    }
}

#[test]
fn globais_perigosos_sao_nil() {
    let mut v = VerbosFalsos::novo();
    rodar(
        r##"
        local globais = { os = os, io = io, require = require, loadstring = loadstring,
            load = load, dofile = dofile, getfenv = getfenv, setfenv = setfenv, debug = debug,
            coroutine = coroutine, package = package, print = print, newproxy = newproxy,
            collectgarbage = collectgarbage, _G = _G }
        for nome, valor in pairs(globais) do
            error("global proibido presente: " .. nome)
        end
        "##,
        &mut v,
    )
    .expect("nenhum global perigoso");
}

#[test]
fn bibliotecas_seguras_funcionam() {
    let mut v = VerbosFalsos::novo();
    rodar(
        r##"
        assert(string.upper("abc") == "ABC")
        assert(string.format("%05.1f", 3.14159) == "003.1")
        assert(("a,b"):find(",", 1, true) == 2)
        local t = { 3, 1, 2 }
        table.sort(t)
        assert(t[1] == 1 and table.concat(t, "") == "123")
        assert(math.floor(2.7) == 2)
        assert(utf8.len("ação") == 4)
        local ok, e = pcall(error, "x")
        assert(not ok)
        assert(tonumber("12") + 1 == 13)
        "##,
        &mut v,
    )
    .expect("libs seguras");
}

#[test]
fn nao_da_para_trocar_os_verbos_nem_as_bibliotecas() {
    for fonte in [
        r##"navegador.clicar = function() end"##,
        r##"util.hoje = function() return "x" end"##,
        r##"navegador = nil; navegador.abrir("x")"##,
        r##"string.rep = nil"##,
    ] {
        let mut v = VerbosFalsos::novo();
        // As tabelas compartilhadas são somente leitura.
        rodar(fonte, &mut v).expect_err(fonte);
    }
}

#[test]
fn verbos_de_segredo_so_recebem_rotulo_e_seletor() {
    let mut v = VerbosFalsos::novo();
    rodar(
        r##"
        navegador.preencher_senha("#senha", "senha:portal-exemplo")
        navegador.preencher_cpf("#cpf")
        navegador.preencher_codigo_do_email("#codigo", { janela_segundos = 90 })
        "##,
        &mut v,
    )
    .expect("ok");
    assert_eq!(
        v.chamadas,
        vec![
            Chamada::PreencherSenha("#senha".into(), "senha:portal-exemplo".into()),
            Chamada::PreencherCpf("#cpf".into()),
            Chamada::PreencherCodigoDoEmail("#codigo".into(), std::time::Duration::from_secs(90)),
        ]
    );
    // A forma das chamadas já prova: não há campo para valor de segredo. Nenhum Debug traz
    // algo além do rótulo, do seletor e da janela.
    let texto = format!("{:?}", v.chamadas);
    assert!(!texto.contains("123456"));
}

#[test]
fn script_nao_consegue_passar_cpf_nem_codigo_aos_verbos_de_segredo() {
    for fonte in [
        r##"navegador.preencher_cpf("#cpf", "12345678901")"##,
        r##"navegador.preencher_codigo_do_email("#c", "123456")"##,
        r##"navegador.preencher_codigo_do_email("#c", { codigo = "123456" })"##,
        r##"navegador.preencher_senha("#s", "rotulo", "senha-literal")"##,
    ] {
        let mut v = VerbosFalsos::novo();
        rodar(fonte, &mut v).expect_err(fonte);
        assert!(v.chamadas.is_empty(), "{fonte}: {:?}", v.chamadas);
    }
}

#[test]
fn bloqueado_termina_o_roteiro_com_o_motivo() {
    let mut v = VerbosFalsos::novo().falhando(
        "clicar",
        ErroDeVerbo::Bloqueado {
            motivo: "botão de pagar".into(),
        },
    );
    let erro = rodar(
        "navegador.abrir('https://portal.exemplo.test/')\nnavegador.clicar('#pagar')\nnavegador.registrar('nunca')",
        &mut v,
    )
    .expect_err("bloqueado");
    assert_eq!(
        erro,
        ErroDeRoteiro::Verbo {
            erro: ErroDeVerbo::Bloqueado {
                motivo: "botão de pagar".into()
            },
            linha: Some(2)
        }
    );
    assert!(erro.to_string().contains("botão de pagar"));
    assert_eq!(v.chamadas.len(), 2, "nada depois do bloqueio");
}

#[test]
fn bloqueado_nao_e_capturavel_com_pcall() {
    let mut v = VerbosFalsos::novo().falhando(
        "clicar",
        ErroDeVerbo::Bloqueado {
            motivo: "transferir".into(),
        },
    );
    let erro = rodar(
        r##"
        pcall(navegador.clicar, "#transferir")
        navegador.clicar("#outro")
        navegador.registrar("depois")
        "##,
        &mut v,
    )
    .expect_err("pcall não salva");
    assert!(matches!(
        erro,
        ErroDeRoteiro::Verbo {
            erro: ErroDeVerbo::Bloqueado { .. },
            ..
        }
    ));
    // Só a primeira chamada chegou ao app; o resto foi recusado pelo host.
    assert_eq!(v.chamadas, vec![Chamada::Clicar("#transferir".into())]);
}

#[test]
fn host_nao_permitido_tambem_termina() {
    let mut v = VerbosFalsos::novo().falhando("abrir", ErroDeVerbo::HostNaoPermitido);
    let erro = rodar(
        "pcall(navegador.abrir, 'https://outro.test/') navegador.registrar('x')",
        &mut v,
    )
    .expect_err("host");
    assert!(matches!(
        erro,
        ErroDeRoteiro::Verbo {
            erro: ErroDeVerbo::HostNaoPermitido,
            ..
        }
    ));
}

#[test]
fn pausa_abortada_devolve_erro_e_termina() {
    let mut v = VerbosFalsos::novo().falhando("pausa", ErroDeVerbo::Abortado);
    let erro = rodar(
        "navegador.pausa('CAPTCHA')\nnavegador.registrar('nunca')",
        &mut v,
    )
    .expect_err("abortado");
    assert_eq!(
        erro,
        ErroDeRoteiro::Verbo {
            erro: ErroDeVerbo::Abortado,
            linha: Some(1)
        }
    );
    assert_eq!(v.chamadas, vec![Chamada::Pausa("CAPTCHA".into())]);
}

#[test]
fn pausa_abortada_nao_se_captura() {
    let mut v = VerbosFalsos::novo().falhando("pausa", ErroDeVerbo::Abortado);
    let erro = rodar(
        "pcall(navegador.pausa, 'x') navegador.registrar('nunca')",
        &mut v,
    )
    .expect_err("abortado");
    assert!(matches!(
        erro,
        ErroDeRoteiro::Verbo {
            erro: ErroDeVerbo::Abortado,
            ..
        }
    ));
    assert_eq!(v.chamadas.len(), 1);
}
