//! O OAuth por loopback de ponta a ponta, com o "navegador" simulado por um cliente TCP e o
//! endpoint de token no servidor falso.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use bancada_rede::testes::{RespostaFalsa, ServidorFalso};
use bancada_rede::{
    Catalogo, ConfiguracaoDaRede, ErroDoRetorno, ErroOAuth, FluxoOAuth, Instante, ProvedorOAuth,
    Rede, Relogio, RelogioDoSistema, Segredo, desafio_pkce,
};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

fn valor(url: &str, chave: &str) -> String {
    let consulta = url.split_once('?').unwrap().1;
    let bruto = consulta
        .split('&')
        .find_map(|p| p.strip_prefix(&format!("{chave}=")))
        .unwrap();
    // Decodificação mínima para o teste.
    let mut saida = String::new();
    let b = bruto.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            saida.push(char::from(
                u8::from_str_radix(&bruto[i + 1..i + 3], 16).unwrap(),
            ));
            i += 3;
        } else {
            saida.push(char::from(b[i]));
            i += 1;
        }
    }
    saida
}

/// O navegador volta ao loopback com `consulta`.
fn navegador_volta(redirect: &str, consulta: &str) -> String {
    let resto = redirect.strip_prefix("http://").unwrap();
    let (autoridade, caminho) = resto.split_at(resto.find('/').unwrap());
    let mut conexao = TcpStream::connect(autoridade).unwrap();
    write!(
        conexao,
        "GET {caminho}?{consulta} HTTP/1.1\r\nHost: {autoridade}\r\n\r\n"
    )
    .unwrap();
    let mut resposta = String::new();
    let _ = conexao.read_to_string(&mut resposta);
    resposta
}

fn prazo() -> Instante {
    RelogioDoSistema.agora().mais(Duration::from_secs(10))
}

#[test]
fn o_codigo_volta_pelo_loopback_e_vira_id_token() {
    let servidor = ServidorFalso::iniciar();
    servidor.rota(
        "/token",
        RespostaFalsa::json(
            r#"{"id_token":"eyJ.fake.jwt","access_token":"ya29","expires_in":3600}"#,
        ),
    );
    let provedor = ProvedorOAuth::novo(
        "https://contas.exemplo.com/auth",
        "https://oauth.exemplo.com/token",
        "cliente-teste",
    )
    .unwrap()
    .escopos(&["openid", "email"]);
    let rede = Rede::abrir(
        ConfiguracaoDaRede::nova(Catalogo::novo().com(provedor.endereco_do_token().unwrap()))
            .rota("oauth.exemplo.com", &servidor.base()),
    )
    .unwrap();

    let fluxo = FluxoOAuth::iniciar(provedor, "/sbf/retorno", "Seu Bolso Feliz").unwrap();
    let url = fluxo.endereco_de_autorizacao().to_owned();
    let estado = valor(&url, "state");
    let redirect = valor(&url, "redirect_uri");
    let desafio = valor(&url, "code_challenge");

    let navegador = std::thread::spawn(move || {
        // O favicon não conta.
        let _ = navegador_volta(&redirect.replace("/sbf/retorno", "/favicon.ico"), "");
        navegador_volta(&redirect, &format!("code=codigo-123&state={estado}"))
    });
    let codigo = fluxo
        .esperar(&RelogioDoSistema, prazo(), &AtomicBool::new(false))
        .unwrap();
    let pagina = navegador.join().unwrap();
    assert!(pagina.contains("Pode voltar ao Seu Bolso Feliz"));
    assert!(!format!("{codigo:?}").contains("codigo-123"));

    let nonce = valor(&url, "nonce");
    let tokens = codigo.trocar(&rede).unwrap();
    assert_eq!(tokens.nonce.expor(), nonce);
    assert_eq!(
        tokens.id_token.as_ref().map(Segredo::expor),
        Some("eyJ.fake.jwt")
    );
    assert!(!format!("{tokens:?}").contains("eyJ"));

    let pedido = &servidor.pedidos()[0];
    let corpo = pedido.corpo_texto();
    assert!(corpo.contains("grant_type=authorization_code"));
    assert!(corpo.contains("code=codigo-123"));
    let verificador = corpo
        .split('&')
        .find_map(|p| p.strip_prefix("code_verifier="))
        .unwrap();
    assert_eq!(desafio_pkce(&Segredo::novo(verificador)), desafio);
}

#[test]
fn estado_que_nao_confere_e_recusado() {
    let fluxo = FluxoOAuth::iniciar(ProvedorOAuth::google("c"), "/r", "App").unwrap();
    let redirect = fluxo.endereco_de_retorno();
    let t = std::thread::spawn(move || navegador_volta(&redirect, "code=x&state=forjado"));
    let erro = fluxo
        .esperar(&RelogioDoSistema, prazo(), &AtomicBool::new(false))
        .unwrap_err();
    t.join().unwrap();
    assert_eq!(erro, ErroOAuth::EstadoNaoConfere);
}

#[test]
fn negar_no_navegador_vira_erro_sanitizado() {
    let fluxo = FluxoOAuth::iniciar(ProvedorOAuth::google("c"), "/r", "App").unwrap();
    let estado = valor(fluxo.endereco_de_autorizacao(), "state");
    let redirect = fluxo.endereco_de_retorno();
    let t = std::thread::spawn(move || {
        navegador_volta(&redirect, &format!("error=access_denied&state={estado}"))
    });
    let erro = fluxo
        .esperar(&RelogioDoSistema, prazo(), &AtomicBool::new(false))
        .unwrap_err();
    t.join().unwrap();
    assert_eq!(
        erro,
        ErroOAuth::Negada {
            erro: "access_denied".to_owned()
        }
    );
}

#[test]
fn sem_retorno_o_prazo_vence_e_cancelar_desiste() {
    let fluxo = FluxoOAuth::iniciar(ProvedorOAuth::google("c"), "/r", "App").unwrap();
    let ja = RelogioDoSistema.agora();
    assert_eq!(
        fluxo
            .esperar(&RelogioDoSistema, ja, &AtomicBool::new(false))
            .unwrap_err(),
        ErroOAuth::Retorno(ErroDoRetorno::Expirado)
    );
    let fluxo = FluxoOAuth::iniciar(ProvedorOAuth::google("c"), "/r", "App").unwrap();
    assert_eq!(
        fluxo
            .esperar(&RelogioDoSistema, prazo(), &AtomicBool::new(true))
            .unwrap_err(),
        ErroOAuth::Retorno(ErroDoRetorno::Cancelado)
    );
}

#[test]
fn codigo_recusado_pelo_provedor() {
    let servidor = ServidorFalso::iniciar();
    servidor.rota(
        "/token",
        RespostaFalsa::erro_json(400, r#"{"error":"invalid_grant"}"#),
    );
    let provedor = ProvedorOAuth::novo(
        "https://contas.exemplo.com/auth",
        "https://oauth.exemplo.com/token",
        "c",
    )
    .unwrap();
    let rede = Rede::abrir(
        ConfiguracaoDaRede::nova(Catalogo::novo().com(provedor.endereco_do_token().unwrap()))
            .rota("oauth.exemplo.com", &servidor.base()),
    )
    .unwrap();
    let fluxo = FluxoOAuth::iniciar(provedor, "/r", "App").unwrap();
    let estado = valor(fluxo.endereco_de_autorizacao(), "state");
    let redirect = fluxo.endereco_de_retorno();
    let t =
        std::thread::spawn(move || navegador_volta(&redirect, &format!("code=c1&state={estado}")));
    let codigo = fluxo
        .esperar(&RelogioDoSistema, prazo(), &AtomicBool::new(false))
        .unwrap();
    t.join().unwrap();
    assert_eq!(codigo.trocar(&rede).unwrap_err(), ErroOAuth::CodigoRecusado);
}
