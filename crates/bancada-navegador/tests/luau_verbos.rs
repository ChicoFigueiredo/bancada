//! Roteiro feliz e cada verbo do host Luau.

mod luau_comum;

use std::time::Duration;

use bancada_navegador::luau::{ErroDeRoteiro, ErroDeVerbo, Limites, executar};
use luau_comum::{Chamada, VerbosFalsos};

fn rodar(
    fonte: &str,
    v: &mut VerbosFalsos,
) -> Result<bancada_navegador::luau::Saida, ErroDeRoteiro> {
    executar(fonte, v, &Limites::default())
}

#[test]
fn roteiro_feliz_completo() {
    let mut v = VerbosFalsos::novo();
    v.textos.insert("#saudacao".into(), "Olá, Fulano".into());
    let fonte = r##"
        navegador.abrir("https://portal.exemplo.test/entrar")
        navegador.preencher("#usuario", "fulano")
        navegador.preencher_senha("#senha", "senha:portal-exemplo")
        navegador.clicar("#entrar")
        if not navegador.esperar("#saudacao") then
            error("não entrou")
        end
        navegador.registrar("entrou: " .. navegador.texto("#saudacao"))
        navegador.clicar("#contracheque")
        local arquivo = navegador.baixar("#baixar-pdf")
        navegador.registrar("baixou " .. arquivo)
    "##;
    let saida = rodar(fonte, &mut v).expect("roteiro feliz");
    assert_eq!(saida.arquivos, vec!["contracheque-2026-11.pdf"]);
    assert_eq!(
        saida.registros,
        vec!["entrou: Olá, Fulano", "baixou contracheque-2026-11.pdf"]
    );
    assert_eq!(
        v.chamadas,
        vec![
            Chamada::Abrir("https://portal.exemplo.test/entrar".into()),
            Chamada::Preencher("#usuario".into(), "fulano".into()),
            Chamada::PreencherSenha("#senha".into(), "senha:portal-exemplo".into()),
            Chamada::Clicar("#entrar".into()),
            Chamada::Esperar("#saudacao".into(), Duration::from_secs(15)),
            Chamada::Texto("#saudacao".into()),
            Chamada::Registrar("entrou: Olá, Fulano".into()),
            Chamada::Clicar("#contracheque".into()),
            Chamada::Baixar("#baixar-pdf".into(), None),
            Chamada::Registrar("baixou contracheque-2026-11.pdf".into()),
        ]
    );
}

#[test]
fn abrir_clicar_e_preencher_repassam_os_argumentos() {
    let mut v = VerbosFalsos::novo();
    rodar(
        r##"navegador.abrir("https://portal.exemplo.test/") navegador.clicar("a.x") navegador.preencher("#c", "oi")"##,
        &mut v,
    )
    .expect("ok");
    assert_eq!(
        v.chamadas,
        vec![
            Chamada::Abrir("https://portal.exemplo.test/".into()),
            Chamada::Clicar("a.x".into()),
            Chamada::Preencher("#c".into(), "oi".into()),
        ]
    );
}

#[test]
fn esperar_padrao_explicito_e_teto() {
    let mut v = VerbosFalsos::novo();
    v.esperar_resultado = Some(false);
    rodar(
        r##"
        assert(navegador.esperar("#a") == false)
        navegador.esperar("#b", 3)
        navegador.esperar("#c", 2.5)
        navegador.esperar("#d", 500)
        navegador.esperar("#e", nil)
        "##,
        &mut v,
    )
    .expect("ok");
    let prazos: Vec<Duration> = v
        .chamadas
        .iter()
        .map(|c| match c {
            Chamada::Esperar(_, d) => *d,
            outro => panic!("chamada inesperada: {outro:?}"),
        })
        .collect();
    assert_eq!(
        prazos,
        vec![
            Duration::from_secs(15),
            Duration::from_secs(3),
            Duration::from_millis(2500),
            Duration::from_secs(120),
            Duration::from_secs(15),
        ]
    );
}

#[test]
fn esperar_recusa_prazo_invalido() {
    for ruim in ["0", "-1", "math.huge * 0", "'dez'", "true"] {
        let mut v = VerbosFalsos::novo();
        let fonte = format!(r##"navegador.esperar("#a", {ruim})"##);
        let erro = rodar(&fonte, &mut v).expect_err(ruim);
        assert!(
            matches!(erro, ErroDeRoteiro::Execucao { .. }),
            "{ruim}: {erro:?}"
        );
        assert!(v.chamadas.is_empty(), "{ruim}");
    }
}

#[test]
fn existe_texto_e_tabela_devolvem_o_que_o_app_deu() {
    let mut v = VerbosFalsos::novo();
    v.existentes.push("#sim".into());
    v.textos.insert("#valor".into(), "R$ 1.234,56".into());
    v.tabelas.insert(
        "table.itens".into(),
        vec![
            vec!["Salário".into(), "5.000,00".into()],
            vec!["INSS".into(), "-550,00".into()],
        ],
    );
    let saida = rodar(
        r##"
        assert(navegador.existe("#sim") == true)
        assert(navegador.existe("#nao") == false)
        assert(navegador.texto("#valor") == "R$ 1.234,56")
        local t = navegador.tabela("table.itens")
        assert(#t == 2 and #t[1] == 2)
        assert(t[2][1] == "INSS" and t[2][2] == "-550,00")
        navegador.registrar("ok")
        "##,
        &mut v,
    )
    .expect("ok");
    assert_eq!(saida.registros, vec!["ok"]);
}

#[test]
fn preencher_cpf_so_recebe_o_seletor() {
    let mut v = VerbosFalsos::novo();
    rodar(r##"navegador.preencher_cpf("#cpf")"##, &mut v).expect("ok");
    assert_eq!(v.chamadas, vec![Chamada::PreencherCpf("#cpf".into())]);
}

#[test]
fn codigo_do_email_janela_padrao_numero_tabela_e_teto() {
    let mut v = VerbosFalsos::novo();
    rodar(
        r##"
        navegador.preencher_codigo_do_email("#a")
        navegador.preencher_codigo_do_email("#b", { janela_segundos = 60 })
        navegador.preencher_codigo_do_email("#c", { janela_segundos = 9999 })
        navegador.preencher_codigo_do_email("#d", 45)
        navegador.preencher_codigo_do_email("#e", {})
        "##,
        &mut v,
    )
    .expect("ok");
    let janelas: Vec<Duration> = v
        .chamadas
        .iter()
        .map(|c| match c {
            Chamada::PreencherCodigoDoEmail(_, d) => *d,
            outro => panic!("{outro:?}"),
        })
        .collect();
    assert_eq!(
        janelas,
        vec![
            Duration::from_secs(120),
            Duration::from_secs(60),
            Duration::from_secs(300),
            Duration::from_secs(45),
            Duration::from_secs(120),
        ]
    );
}

#[test]
fn codigo_do_email_recusa_remetente_e_formato_vindos_do_script() {
    let mut v = VerbosFalsos::novo();
    let erro = rodar(
        r##"navegador.preencher_codigo_do_email("#a", { remetente = "x@exemplo.test" })"##,
        &mut v,
    )
    .expect_err("remetente é do manifesto");
    assert!(erro.to_string().contains("manifesto"), "{erro}");
    assert!(v.chamadas.is_empty());
}

#[test]
fn baixar_com_nome_e_saida_acumula_arquivos() {
    let mut v = VerbosFalsos::novo();
    let saida = rodar(
        r##"
        local a = navegador.baixar("#a", "extrato.pdf")
        local b = navegador.baixar("#b")
        assert(a == "extrato.pdf")
        "##,
        &mut v,
    )
    .expect("ok");
    assert_eq!(
        saida.arquivos,
        vec!["extrato.pdf", "contracheque-2026-11.pdf"]
    );
    assert_eq!(
        v.chamadas[0],
        Chamada::Baixar("#a".into(), Some("extrato.pdf".into()))
    );
}

#[test]
fn baixar_recusa_nome_com_pasta() {
    for ruim in ["../fora.pdf", "a/b.pdf", "a\\b.pdf", "..", ""] {
        let mut v = VerbosFalsos::novo();
        let fonte = format!(
            r##"navegador.baixar("#a", "{}")"##,
            ruim.replace('\\', "\\\\")
        );
        let erro = rodar(&fonte, &mut v).expect_err(ruim);
        assert!(
            matches!(erro, ErroDeRoteiro::Execucao { .. }),
            "{ruim}: {erro:?}"
        );
        assert!(v.chamadas.is_empty(), "{ruim}");
    }
}

#[test]
fn pausa_e_registrar() {
    let mut v = VerbosFalsos::novo();
    let saida = rodar(
        r##"navegador.pausa("Resolva o CAPTCHA e continue") navegador.registrar("seguiu")"##,
        &mut v,
    )
    .expect("ok");
    assert_eq!(saida.registros, vec!["seguiu"]);
    assert_eq!(
        v.chamadas[0],
        Chamada::Pausa("Resolva o CAPTCHA e continue".into())
    );
}

#[test]
fn registrar_trunca_mensagem_enorme() {
    let mut v = VerbosFalsos::novo();
    let saida = rodar(r##"navegador.registrar(string.rep("a", 10000))"##, &mut v).expect("ok");
    assert_eq!(saida.registros[0].chars().count(), 2000);
}

#[test]
fn util_hoje_e_somar_meses() {
    let mut v = VerbosFalsos::novo();
    let saida = rodar(
        r##"
        local h = util.hoje()
        local m = util.somar_meses(h, 1)
        local d = util.somar_meses(h, -3)
        navegador.registrar(h .. " " .. m)
        "##,
        &mut v,
    )
    .expect("ok");
    assert_eq!(saida.registros, vec!["2026-11-24 2026-12-24"]);
    assert_eq!(
        v.chamadas[1..],
        [
            Chamada::SomarMeses("2026-11-24".into(), 1),
            Chamada::SomarMeses("2026-11-24".into(), -3),
            Chamada::Registrar("2026-11-24 2026-12-24".into()),
        ]
    );
}

#[test]
fn somar_meses_valida_n() {
    for ruim in ["1.5", "'1'", "nil", "5000", "-5000", "math.huge"] {
        let mut v = VerbosFalsos::novo();
        let fonte = format!(r##"util.somar_meses("2026-11-24", {ruim})"##);
        let erro = rodar(&fonte, &mut v).expect_err(ruim);
        assert!(
            matches!(erro, ErroDeRoteiro::Execucao { .. }),
            "{ruim}: {erro:?}"
        );
        assert!(v.chamadas.is_empty(), "{ruim}");
    }
    let mut v = VerbosFalsos::novo();
    rodar(r##"util.somar_meses("2026-11-24", 2.0)"##, &mut v)
        .expect("inteiro em ponto flutuante vale");
}

#[test]
fn erros_de_tipo_sao_claros_e_nao_chamam_o_verbo() {
    let casos = [
        (
            r##"navegador.abrir(123)"##,
            "navegador.abrir",
            "argumento 1 (url)",
        ),
        (
            r##"navegador.abrir()"##,
            "navegador.abrir",
            "falta o argumento 1",
        ),
        (r##"navegador.clicar({})"##, "navegador.clicar", "um texto"),
        (
            r##"navegador.preencher("#a", 42)"##,
            "navegador.preencher",
            "argumento 2 (texto)",
        ),
        (
            r##"navegador.preencher_senha("#a", nil)"##,
            "navegador.preencher_senha",
            "falta o argumento 2",
        ),
        (
            r##"navegador.preencher_senha("#a", {})"##,
            "navegador.preencher_senha",
            "um texto",
        ),
        (
            r##"navegador.preencher_cpf("#a", "12345678901")"##,
            "navegador.preencher_cpf",
            "no máximo 1",
        ),
        (
            r##"navegador:clicar("#a")"##,
            "navegador.clicar",
            "2 argumentos",
        ),
        (
            r##"navegador.registrar(navegador.clicar)"##,
            "navegador.registrar",
            "função",
        ),
        (
            r##"navegador.baixar("#a", 7)"##,
            "navegador.baixar",
            "argumento 2 (nome)",
        ),
        (r##"navegador.pausa(true)"##, "navegador.pausa", "booleano"),
        (r##"util.hoje(1)"##, "util.hoje", "no máximo 0"),
    ];
    for (fonte, verbo, trecho) in casos {
        let mut v = VerbosFalsos::novo();
        let erro = rodar(fonte, &mut v).expect_err(fonte);
        let ErroDeRoteiro::Execucao { mensagem, linha } = &erro else {
            panic!("{fonte}: {erro:?}");
        };
        assert!(mensagem.contains(verbo), "{fonte}: {mensagem}");
        assert!(mensagem.contains(trecho), "{fonte}: {mensagem}");
        assert_eq!(*linha, Some(1), "{fonte}");
        assert!(v.chamadas.is_empty(), "{fonte}: {:?}", v.chamadas);
    }
}

#[test]
fn erro_do_script_traz_a_linha() {
    let mut v = VerbosFalsos::novo();
    let erro = rodar("local x = 1\n\nerror('deu ruim')\n", &mut v).expect_err("erro");
    assert_eq!(
        erro,
        ErroDeRoteiro::Execucao {
            mensagem: "deu ruim".into(),
            linha: Some(3)
        }
    );
    assert!(erro.to_string().contains("linha 3"));
}

#[test]
fn erro_de_sintaxe_traz_a_linha() {
    let mut v = VerbosFalsos::novo();
    let erro = rodar("local x = 1\nlocal = = 2\n", &mut v).expect_err("sintaxe");
    let ErroDeRoteiro::Sintaxe { linha, .. } = erro else {
        panic!("{erro:?}");
    };
    assert_eq!(linha, Some(2));
}

#[test]
fn falha_de_verbo_capturavel_com_pcall() {
    let mut v =
        VerbosFalsos::novo().falhando("clicar", ErroDeVerbo::Falhou("seletor não achado".into()));
    let saida = rodar(
        r##"
        local ok = pcall(navegador.clicar, "#some")
        assert(ok == false)
        navegador.clicar("#outro")
        navegador.registrar("seguiu")
        "##,
        &mut v,
    )
    .expect("pcall captura Falhou");
    assert_eq!(saida.registros, vec!["seguiu"]);
}

#[test]
fn falha_de_verbo_nao_capturada_termina_com_o_erro_e_a_linha() {
    let mut v = VerbosFalsos::novo().falhando("baixar", ErroDeVerbo::Prazo("download".into()));
    let erro = rodar(
        "local a = 1\nnavegador.baixar('#a')\nnavegador.registrar('nunca')",
        &mut v,
    )
    .expect_err("prazo");
    assert_eq!(
        erro,
        ErroDeRoteiro::Verbo {
            erro: ErroDeVerbo::Prazo("download".into()),
            linha: Some(2)
        }
    );
    assert!(!v.chamadas.contains(&Chamada::Registrar("nunca".into())));
}

#[test]
fn execucoes_sao_isoladas() {
    let mut v = VerbosFalsos::novo();
    rodar("segredo_global = 1", &mut v).expect("ok");
    rodar("assert(segredo_global == nil)", &mut v).expect("outro VM, sem o global");
}

#[test]
fn esperar_sumir_devolve_true_quando_o_elemento_nao_esta_e_false_se_ele_fica() {
    let mut v = VerbosFalsos::novo();
    v.existentes.push("#fica".into());
    rodar(
        r##"
        assert(navegador.esperar_sumir("#nunca-houve", 5) == true)
        assert(navegador.esperar_sumir("#fica", 1) == false)
        "##,
        &mut v,
    )
    .expect("ok");
}

#[test]
fn esperar_sumir_recusa_prazo_nao_positivo_e_argumento_errado() {
    for ruim in [
        r##"navegador.esperar_sumir("#x", 0)"##,
        "navegador.esperar_sumir(42)",
        r##"navegador.esperar_sumir("#x", 1, 2)"##,
    ] {
        let mut v = VerbosFalsos::novo();
        assert!(rodar(ruim, &mut v).is_err(), "{ruim}");
    }
}
