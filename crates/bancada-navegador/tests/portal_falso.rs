//! O portal de contracheque de mentira, por HTTP cru (sem Chrome): login, segunda etapa
//! (e-mail, token, CAPTCHA), lista de meses, download do PDF e os distratores.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use bancada_navegador::site_falso::{
    Config, HOST_LOGICO_DO_PORTAL, MfaDoPortal, PortalDeContracheque, pdf_do_contracheque,
};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, SystemTime};

const CPF: &str = "98765432100";
const SENHA: &str = "SenhaFicticia#7700";

struct Resposta {
    status: u16,
    cabecalho: String,
    corpo: Vec<u8>,
}

impl Resposta {
    fn texto(&self) -> String {
        String::from_utf8_lossy(&self.corpo).into_owned()
    }
    fn local(&self) -> Option<String> {
        self.cabecalho_de("location")
    }
    fn cabecalho_de(&self, nome: &str) -> Option<String> {
        self.cabecalho.lines().find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.trim()
                .eq_ignore_ascii_case(nome)
                .then(|| v.trim().to_owned())
        })
    }
    /// O par `nome=valor` do `Set-Cookie`, pronto para o cabeçalho `Cookie`.
    fn cookie(&self) -> Option<String> {
        self.cabecalho_de("set-cookie")
            .map(|c| c.split(';').next().unwrap_or("").to_owned())
    }
}

fn pedir(porta: u16, metodo: &str, caminho: &str, corpo: &str, cookies: &str) -> Resposta {
    let mut c = TcpStream::connect(("127.0.0.1", porta)).unwrap();
    c.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    write!(
        c,
        "{metodo} {caminho} HTTP/1.1\r\nHost: {HOST_LOGICO_DO_PORTAL}\r\nCookie: {cookies}\r\n\
         Content-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\
         Connection: close\r\n\r\n{corpo}",
        corpo.len()
    )
    .unwrap();
    let mut bruto = Vec::new();
    c.read_to_end(&mut bruto).unwrap();
    let fim = bruto.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let cabecalho = String::from_utf8_lossy(&bruto[..fim]).into_owned();
    let status = cabecalho.split(' ').nth(1).unwrap().parse().unwrap();
    Resposta {
        status,
        cabecalho,
        corpo: bruto[fim + 4..].to_vec(),
    }
}

fn portal(mfa: MfaDoPortal) -> PortalDeContracheque {
    PortalDeContracheque::iniciar(Config::nova(CPF, SENHA).com_mfa(mfa).com_codigo("482913"))
        .unwrap()
}

fn login(p: &PortalDeContracheque) -> String {
    let r = pedir(
        p.porta(),
        "POST",
        "/entrar",
        &format!("cpf={CPF}&senha={}", SENHA.replace('#', "%23")),
        "",
    );
    assert_eq!(r.status, 303);
    assert_eq!(r.local().as_deref(), Some("/codigo"));
    r.cookie().unwrap()
}

/// Login e código certos: devolve os cookies da sessão completa.
fn entrar_de_vez(p: &PortalDeContracheque) -> String {
    let sessao = login(p);
    let r = pedir(p.porta(), "POST", "/codigo", "codigo=482913", &sessao);
    assert_eq!(r.local().as_deref(), Some("/contracheques"));
    format!("{sessao}; {}", r.cookie().unwrap())
}

#[test]
fn formulario_de_login_tem_os_campos() {
    let p = portal(MfaDoPortal::Email);
    let r = pedir(p.porta(), "GET", "/entrar", "", "");
    let h = r.texto();
    assert_eq!(r.status, 200);
    assert!(h.contains("id=\"cpf\""));
    assert!(h.contains("id=\"senha\"") && h.contains("type=\"password\""));
    assert!(h.contains("id=\"entrar\""));
    assert_eq!(p.host_logico(), "portal.exemplo.test");
    assert!(p.url_base().starts_with("http://127.0.0.1:"));
}

#[test]
fn login_errado_volta_com_erro_e_nao_gera_codigo() {
    let p = portal(MfaDoPortal::Email);
    let r = pedir(p.porta(), "POST", "/entrar", "cpf=111&senha=errada", "");
    assert_eq!(r.local().as_deref(), Some("/entrar?erro=1"));
    assert!(r.cookie().is_none());
    assert!(
        pedir(p.porta(), "GET", "/entrar?erro=1", "", "")
            .texto()
            .contains("id=\"erro\"")
    );
    assert_eq!(p.logins_recusados(), 1);
    assert_eq!(p.codigo_gerado(), None);
    assert_eq!(p.instante_do_codigo(), None);
}

#[test]
fn login_certo_gera_codigo_e_mostra_a_pagina_do_codigo() {
    let p = portal(MfaDoPortal::Email);
    let antes = SystemTime::now();
    let sessao = login(&p);
    assert_eq!(p.logins(), 1);
    assert_eq!(p.codigo_gerado().as_deref(), Some("482913"));
    assert!(p.instante_do_codigo().unwrap() >= antes);
    let h = pedir(p.porta(), "GET", "/codigo", "", &sessao).texto();
    assert!(h.contains("Enviamos um código por e-mail"));
    assert!(h.contains("id=\"codigo\"") && h.contains("id=\"confirmar\""));
    // Sem sessão, volta ao login.
    assert_eq!(
        pedir(p.porta(), "GET", "/codigo", "", "")
            .local()
            .as_deref(),
        Some("/entrar")
    );
}

#[test]
fn codigo_aleatorio_tem_seis_digitos() {
    let p = PortalDeContracheque::iniciar(Config::nova(CPF, SENHA)).unwrap();
    login(&p);
    let c = p.codigo_gerado().unwrap();
    assert_eq!(c.len(), 6);
    assert!(c.chars().all(|x| x.is_ascii_digit()));
}

#[test]
fn codigo_errado_segue_em_codigo_e_certo_libera_a_lista() {
    let p = portal(MfaDoPortal::Email);
    let sessao = login(&p);
    // A lista fecha sem a segunda etapa.
    assert_eq!(
        pedir(p.porta(), "GET", "/contracheques", "", &sessao)
            .local()
            .as_deref(),
        Some("/codigo")
    );
    let r = pedir(p.porta(), "POST", "/codigo", "codigo=000000", &sessao);
    assert_eq!(r.local().as_deref(), Some("/codigo?erro=1"));
    assert_eq!(p.codigos_recusados(), 1);
    assert!(
        pedir(p.porta(), "GET", "/codigo?erro=1", "", &sessao)
            .texto()
            .contains("id=\"erro\"")
    );
    let r = pedir(p.porta(), "POST", "/codigo", "codigo=482913", &sessao);
    assert_eq!(r.local().as_deref(), Some("/contracheques"));
    let cookies = format!("{sessao}; {}", r.cookie().unwrap());
    assert_eq!(
        pedir(p.porta(), "GET", "/contracheques", "", &cookies).status,
        200
    );
}

#[test]
fn lista_tem_a_tabela_e_um_link_por_mes() {
    let p = portal(MfaDoPortal::Email);
    let cookies = entrar_de_vez(&p);
    let h = pedir(p.porta(), "GET", "/contracheques", "", &cookies).texto();
    assert!(h.contains("<table id=\"lista\">"));
    assert!(h.contains("Outubro/2026") && h.contains("Novembro/2026"));
    assert!(
        h.contains("class=\"baixar\" id=\"baixar-2026-10\" href=\"/contracheque/2026-10.pdf\"")
    );
    assert!(h.contains("href=\"/contracheque/2026-11.pdf\""));
    assert!(!h.contains("2026-12"));
}

#[test]
fn download_devolve_o_pdf_com_attachment() {
    let p = portal(MfaDoPortal::Email);
    let cookies = entrar_de_vez(&p);
    assert!(!p.baixou("2026-10"));
    let r = pedir(p.porta(), "GET", "/contracheque/2026-10.pdf", "", &cookies);
    assert_eq!(r.status, 200);
    assert!(
        r.cabecalho_de("content-disposition")
            .unwrap()
            .starts_with("attachment")
    );
    assert_eq!(
        r.cabecalho_de("content-type").as_deref(),
        Some("application/pdf")
    );
    assert!(r.corpo.starts_with(b"%PDF-"));
    let t = r.texto();
    assert!(t.contains("Contracheque FIC\\315TICIO 2026-10"));
    assert!(t.trim_end().ends_with("%%EOF"));
    assert_eq!(r.corpo, pdf_do_contracheque("2026-10"));
    assert!(p.baixou("2026-10") && !p.baixou("2026-11"));
    assert_eq!(p.downloads(), 1);
    // Determinístico por mês, e diferente entre meses.
    assert_ne!(
        pdf_do_contracheque("2026-10"),
        pdf_do_contracheque("2026-11")
    );
}

#[test]
fn xref_do_pdf_aponta_para_os_objetos() {
    let pdf = String::from_utf8(pdf_do_contracheque("2026-11")).unwrap();
    let inicio = pdf.find("xref\n").unwrap();
    let posicoes: Vec<usize> = pdf[inicio..]
        .lines()
        .skip(3)
        .take(5)
        .map(|l| l[..10].parse().unwrap())
        .collect();
    for (i, pos) in posicoes.iter().enumerate() {
        assert!(pdf[*pos..].starts_with(&format!("{} 0 obj", i + 1)));
    }
    let start: usize = pdf
        .split("startxref\n")
        .nth(1)
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(start, inicio);
}

#[test]
fn download_exige_sessao_completa_e_mes_publicado() {
    let p = portal(MfaDoPortal::Email);
    let sessao = login(&p);
    // Só com a primeira etapa, não baixa.
    assert_eq!(
        pedir(p.porta(), "GET", "/contracheque/2026-10.pdf", "", &sessao).status,
        303
    );
    assert_eq!(
        pedir(p.porta(), "GET", "/contracheque/2026-10.pdf", "", "").status,
        303
    );
    let cookies = entrar_de_vez(&p);
    // Mês que ainda não saiu.
    assert_eq!(
        pedir(p.porta(), "GET", "/contracheque/2026-12.pdf", "", &cookies).status,
        404
    );
    assert_eq!(p.downloads(), 0);
}

#[test]
fn meses_configuraveis_e_invalidos_sao_ignorados() {
    let p = PortalDeContracheque::iniciar(
        Config::nova(CPF, SENHA)
            .com_codigo("111111")
            .com_meses(&["2026-10", "2026-13", "lixo"]),
    )
    .unwrap();
    let sessao = login(&p);
    let r = pedir(p.porta(), "POST", "/codigo", "codigo=111111", &sessao);
    let cookies = format!("{sessao}; {}", r.cookie().unwrap());
    let h = pedir(p.porta(), "GET", "/contracheques", "", &cookies).texto();
    assert!(h.contains("2026-10") && !h.contains("2026-13") && !h.contains("lixo"));
}

#[test]
fn modo_token_pede_o_app_autenticador_e_a_liberacao_de_teste_destrava() {
    let p = portal(MfaDoPortal::Token);
    let sessao = login(&p);
    let h = pedir(p.porta(), "GET", "/codigo", "", &sessao).texto();
    assert!(h.contains("app autenticador"));
    assert!(h.contains("id=\"token\"") && h.contains("id=\"confirmar\""));
    assert!(!h.contains("id=\"codigo\""));
    assert_eq!(
        pedir(p.porta(), "GET", "/contracheques", "", &sessao)
            .local()
            .as_deref(),
        Some("/codigo")
    );
    assert_eq!(
        pedir(p.porta(), "GET", "/_teste/liberado", "", "").texto(),
        "0"
    );
    // O usuário "resolve" pelo endpoint de teste.
    assert_eq!(
        pedir(p.porta(), "POST", "/_teste/liberar", "", "").status,
        204
    );
    assert_eq!(
        pedir(p.porta(), "GET", "/_teste/liberado", "", "").texto(),
        "1"
    );
    assert_eq!(
        pedir(p.porta(), "GET", "/contracheques", "", &sessao).status,
        200
    );
}

#[test]
fn modo_token_tambem_aceita_o_token_certo_digitado() {
    let p = portal(MfaDoPortal::Token);
    let sessao = login(&p);
    let errado = pedir(p.porta(), "POST", "/codigo", "token=000000", &sessao);
    assert_eq!(errado.local().as_deref(), Some("/codigo?erro=1"));
    let certo = pedir(p.porta(), "POST", "/codigo", "token=482913", &sessao);
    assert_eq!(certo.local().as_deref(), Some("/contracheques"));
}

#[test]
fn modo_captcha_mostra_a_palavra_e_so_a_liberacao_passa() {
    let p = portal(MfaDoPortal::Captcha);
    let sessao = login(&p);
    let h = pedir(p.porta(), "GET", "/codigo", "", &sessao).texto();
    assert!(h.contains("captcha") && h.contains("Não sou um robô"));
    // Nem o código certo passa: só o humano.
    let r = pedir(p.porta(), "POST", "/codigo", "codigo=482913", &sessao);
    assert_eq!(r.local().as_deref(), Some("/codigo?erro=1"));
    assert_eq!(
        pedir(p.porta(), "GET", "/contracheques", "", &sessao)
            .local()
            .as_deref(),
        Some("/codigo")
    );
    p.liberar();
    assert_eq!(
        pedir(p.porta(), "GET", "/contracheques", "", &sessao).status,
        200
    );
}

#[test]
fn distratores_existem_e_cada_acesso_fica_registrado() {
    let p = portal(MfaDoPortal::Email);
    let cookies = entrar_de_vez(&p);
    let h = pedir(p.porta(), "GET", "/contracheques", "", &cookies).texto();
    assert!(h.contains("Pagar boleto") && h.contains("Alterar cadastro"));
    // Navegar e baixar não toca os distratores.
    pedir(p.porta(), "GET", "/contracheque/2026-11.pdf", "", &cookies);
    assert!(p.acessos_proibidos().is_empty());
    // Se alguém clicar, o registro mostra.
    pedir(p.porta(), "POST", "/boleto/pagar", "", &cookies);
    pedir(p.porta(), "GET", "/cadastro/alterar", "", &cookies);
    assert_eq!(
        p.acessos_proibidos(),
        vec!["POST /boleto/pagar", "GET /cadastro/alterar"]
    );
}

#[test]
fn encerra_o_servidor_no_drop() {
    let porta = {
        let p = portal(MfaDoPortal::Email);
        p.porta()
    };
    assert!(TcpStream::connect(("127.0.0.1", porta)).is_err());
}
