//! O portão contra o servidor falso: catálogo, autorização, cache com ETag, limites, queda.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use bancada_rede::testes::{RespostaFalsa, ServidorFalso};
use bancada_rede::{
    Autorizacao, Catalogo, ConfiguracaoDaRede, CorpoDeErro, Endereco, ErroDeRede,
    FonteDeAutorizacao, Frescor, Instante, Pedido, PoliticaDeRepeticao, Rede, RelogioFalso,
    Segredo, SituacaoDoHost,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const HOST: &str = "sbf.exemplo.com.br";
const BASE: &str = "https://sbf.exemplo.com.br/api";

#[derive(Debug, Default)]
struct Chaveiro {
    token: Mutex<Option<Segredo>>,
    recusas: AtomicUsize,
    ultimo_corpo: Mutex<Option<String>>,
}

impl FonteDeAutorizacao for Chaveiro {
    fn autorizacao(&self, endereco: &Endereco) -> Option<Autorizacao> {
        assert_eq!(endereco.nome(), "servidor");
        self.token
            .lock()
            .unwrap()
            .as_ref()
            .and_then(Autorizacao::portador)
    }

    fn recusada(&self, _endereco: &Endereco, _status: u16, corpo: &CorpoDeErro) {
        self.recusas.fetch_add(1, Ordering::SeqCst);
        *self.ultimo_corpo.lock().unwrap() = corpo.texto().map(str::to_owned);
    }
}

fn catalogo() -> Catalogo {
    Catalogo::novo()
        .com(Endereco::novo("servidor", BASE).unwrap().com_autorizacao())
        .com(Endereco::novo("publico", "https://sbf.exemplo.com.br/publico").unwrap())
}

fn relogio() -> Arc<RelogioFalso> {
    Arc::new(RelogioFalso::novo(Instante::de_milissegundos(
        1_800_000_000_000,
    )))
}

fn rede(servidor: &ServidorFalso, chaveiro: Arc<Chaveiro>, relogio: Arc<RelogioFalso>) -> Rede {
    Rede::abrir(
        ConfiguracaoDaRede::nova(catalogo())
            .rota(HOST, &servidor.base())
            .relogio(relogio)
            .autorizacao(chaveiro)
            .cache_em_memoria()
            .semente(7)
            .repeticao(PoliticaDeRepeticao {
                repeticoes: 2,
                espera_base: Duration::from_millis(10),
                espera_maxima: Duration::from_millis(20),
            }),
    )
    .unwrap()
}

#[test]
fn fora_do_catalogo_e_http_sem_tls_nem_saem() {
    let servidor = ServidorFalso::iniciar();
    let rede = rede(&servidor, Arc::default(), relogio());
    for url in [
        "http://sbf.exemplo.com.br/api/v1/estado",
        "https://outro.exemplo.com.br/api/v1/estado",
        "https://sbf.exemplo.com.br/apix",
    ] {
        let erro = rede.executar(&Pedido::get(url)).unwrap_err();
        assert!(
            matches!(erro, ErroDeRede::EnderecoForaDoCatalogo { .. }),
            "{url}: {erro:?}"
        );
    }
    assert!(servidor.pedidos().is_empty());
}

#[test]
fn a_rota_dos_testes_so_aceita_loopback_ou_https() {
    let config = ConfiguracaoDaRede::nova(catalogo()).rota(HOST, "http://exemplo.org:80");
    assert!(Rede::abrir(config).is_err());
}

#[test]
fn a_credencial_so_vai_para_o_endereco_que_a_recebe_e_nunca_sai_sem_ela() {
    let servidor = ServidorFalso::iniciar();
    servidor.rota("/api/v1/estado", RespostaFalsa::json("{}"));
    servidor.rota("/publico/x", RespostaFalsa::json("{}"));
    let chaveiro = Arc::new(Chaveiro::default());
    let rede = rede(&servidor, chaveiro.clone(), relogio());

    // Sem credencial: nada sai.
    let erro = rede
        .executar(&Pedido::get(format!("{BASE}/v1/estado")).autorizado())
        .unwrap_err();
    assert!(matches!(erro, ErroDeRede::SemCredencial { .. }));
    assert!(servidor.pedidos().is_empty());

    *chaveiro.token.lock().unwrap() = Some(Segredo::novo("tok-estacao-1"));
    rede.executar(&Pedido::get(format!("{BASE}/v1/estado")).autorizado())
        .unwrap();
    assert_eq!(
        servidor.pedidos()[0].cabecalho("authorization"),
        Some("Bearer tok-estacao-1")
    );

    // Endereço que não recebe credencial: recusado, nada sai.
    let erro = rede
        .executar(&Pedido::get("https://sbf.exemplo.com.br/publico/x").autorizado())
        .unwrap_err();
    assert!(matches!(erro, ErroDeRede::EnderecoSemAutorizacao { .. }));
    // Sem pedir: vai sem o cabeçalho.
    rede.executar(&Pedido::get("https://sbf.exemplo.com.br/publico/x"))
        .unwrap();
    let ultimo = servidor.pedidos().pop().unwrap();
    assert_eq!(ultimo.cabecalho("authorization"), None);
}

#[test]
fn a_recusa_da_credencial_avisa_a_fonte_com_o_corpo() {
    let servidor = ServidorFalso::iniciar();
    servidor.rota(
        "/api/v1/estado",
        RespostaFalsa::erro_json(401, r#"{"codigo":"estacao_revogada"}"#),
    );
    let chaveiro = Arc::new(Chaveiro::default());
    *chaveiro.token.lock().unwrap() = Some(Segredo::novo("tok"));
    let rede = rede(&servidor, chaveiro.clone(), relogio());
    let erro = rede
        .executar(&Pedido::get(format!("{BASE}/v1/estado")).autorizado())
        .unwrap_err();
    let ErroDeRede::AutorizacaoRecusada { status, corpo } = &erro else {
        panic!("{erro:?}");
    };
    assert_eq!(*status, 401);
    assert_eq!(corpo.texto(), Some(r#"{"codigo":"estacao_revogada"}"#));
    assert_eq!(chaveiro.recusas.load(Ordering::SeqCst), 1);
    assert_eq!(
        chaveiro.ultimo_corpo.lock().unwrap().as_deref(),
        Some(r#"{"codigo":"estacao_revogada"}"#)
    );
    // O token não aparece no Debug do erro nem da rede.
    assert!(!format!("{erro:?}{rede:?}").contains("tok"));
}

#[test]
fn o_pedido_condicional_de_quem_tem_o_proprio_cache() {
    let servidor = ServidorFalso::iniciar();
    servidor.rota_em_sequencia(
        "/api/v1/livro/contas",
        vec![
            RespostaFalsa::json("[1]").cabecalho("etag", "\"v1\""),
            RespostaFalsa::status(304).cabecalho("etag", "\"v1\""),
        ],
    );
    let rede = rede(&servidor, Arc::default(), relogio());
    let url = format!("{BASE}/v1/livro/contas");
    let primeira = rede.executar(&Pedido::get(&url)).unwrap();
    assert_eq!(primeira.etag.as_deref(), Some("\"v1\""));
    assert_eq!(primeira.frescor, Frescor::Rede);
    let segunda = rede
        .executar(&Pedido::get(&url).se_mudou_desde("\"v1\""))
        .unwrap();
    assert_eq!(segunda.frescor, Frescor::NaoMudou);
    assert!(segunda.corpo.is_empty());
    assert_eq!(
        servidor.pedidos()[1].cabecalho("if-none-match"),
        Some("\"v1\"")
    );
}

#[test]
fn o_cache_do_portao_revalida_e_serve_vencido_sem_rede() {
    let servidor = ServidorFalso::iniciar();
    servidor.rota_em_sequencia(
        "/api/v1/estado",
        vec![
            RespostaFalsa::json("{\"a\":1}").cabecalho("etag", "\"e1\""),
            RespostaFalsa::status(304),
        ],
    );
    let relogio = relogio();
    let rede = rede(&servidor, Arc::default(), relogio.clone());
    let pedido = Pedido::get(format!("{BASE}/v1/estado")).guardar_por(Duration::from_secs(60));
    assert_eq!(rede.executar(&pedido).unwrap().frescor, Frescor::Rede);
    // Dentro da validade: nem toca a rede.
    assert_eq!(rede.executar(&pedido).unwrap().frescor, Frescor::Cache);
    assert_eq!(servidor.pedidos().len(), 1);
    relogio.avancar(Duration::from_secs(61));
    let revalidada = rede.executar(&pedido).unwrap();
    assert_eq!(revalidada.frescor, Frescor::Revalidada);
    assert_eq!(revalidada.corpo, b"{\"a\":1}");

    // O servidor cai: o cache vencido volta marcado, com a causa.
    drop(servidor);
    relogio.avancar(Duration::from_secs(120));
    let vencida = rede.executar(&pedido).unwrap();
    assert!(matches!(vencida.frescor, Frescor::Vencida { .. }));
    assert_eq!(vencida.idade, Duration::from_secs(120));
    assert!(rede.executar(&pedido.clone().sem_vencido()).is_err());
    let lida = rede.ler_cache(&pedido).unwrap().unwrap();
    assert_eq!(lida.corpo, b"{\"a\":1}");
    assert!(rede.estado().sem_rede(HOST));
}

#[test]
fn erro_temporario_se_repete_so_no_get() {
    let servidor = ServidorFalso::iniciar();
    servidor.rota_em_sequencia(
        "/api/v1/estado",
        vec![RespostaFalsa::status(503), RespostaFalsa::json("{}")],
    );
    servidor.rota("/api/v1/alteracoes", RespostaFalsa::status(503));
    let rede = rede(&servidor, Arc::default(), relogio());
    rede.executar(&Pedido::get(format!("{BASE}/v1/estado")))
        .unwrap();
    assert_eq!(servidor.pedidos().len(), 2);
    let erro = rede
        .executar(&Pedido::post(format!("{BASE}/v1/alteracoes")).json(b"{}".to_vec()))
        .unwrap_err();
    assert_eq!(erro.status_http(), Some(503));
    assert_eq!(servidor.pedidos().len(), 3, "POST não se repete");
}

#[test]
fn pausa_pedida_pelo_servidor_segura_o_host() {
    let servidor = ServidorFalso::iniciar();
    servidor.rota(
        "/api/v1/estado",
        RespostaFalsa::status(429).cabecalho("retry-after", "30"),
    );
    let relogio = relogio();
    let rede = rede(&servidor, Arc::default(), relogio.clone());
    let erro = rede
        .executar(&Pedido::get(format!("{BASE}/v1/estado")))
        .unwrap_err();
    assert!(matches!(erro, ErroDeRede::Limitado { .. }));
    assert!(matches!(
        rede.estado().situacao(HOST),
        SituacaoDoHost::Limitado { .. }
    ));
    // Limitado: o pedido seguinte nem sai.
    let antes = servidor.pedidos().len();
    assert!(
        rede.executar(&Pedido::get(format!("{BASE}/v1/estado")))
            .is_err()
    );
    assert_eq!(servidor.pedidos().len(), antes);
}

#[test]
fn corte_no_meio_da_resposta_e_falta_de_rede() {
    let servidor = ServidorFalso::iniciar();
    servidor.rota(
        "/api/v1/livro/movimentacoes",
        RespostaFalsa::json(&"x".repeat(10_000)).cortar_apos(100),
    );
    let rede = rede(&servidor, Arc::default(), relogio());
    let erro = rede
        .executar(&Pedido::get(format!("{BASE}/v1/livro/movimentacoes")))
        .unwrap_err();
    assert!(erro.e_falta_de_rede(), "{erro:?}");
}

#[test]
fn buscar_nao_bloqueia_e_entrega_pelo_pendente() {
    let servidor = ServidorFalso::iniciar();
    servidor.rota("/api/v1/estado", RespostaFalsa::json("{\"ok\":1}"));
    let rede = rede(&servidor, Arc::default(), relogio());
    let pendente = rede.buscar(Pedido::get(format!("{BASE}/v1/estado")));
    let resposta = pendente.esperar().unwrap();
    assert_eq!(resposta.texto(), Some("{\"ok\":1}"));
    let mut recusado = rede.buscar(Pedido::get("http://sbf.exemplo.com.br/api"));
    assert!(matches!(recusado.tentar_pegar(), Some(Err(_))));
    assert!(recusado.tentar_pegar().is_none());
    let ops = rede.operacoes_recentes();
    assert_eq!(ops.len(), 2);
    assert_eq!(ops[0].caminho, "/api/v1/estado");
}

#[test]
fn redirecionamento_nunca_e_seguido() {
    let servidor = ServidorFalso::iniciar();
    servidor.rota(
        "/api/v1/estado",
        RespostaFalsa::status(302).cabecalho("location", "https://outro.exemplo/x"),
    );
    let rede = rede(&servidor, Arc::default(), relogio());
    let erro = rede
        .executar(&Pedido::get(format!("{BASE}/v1/estado")))
        .unwrap_err();
    assert_eq!(erro, ErroDeRede::RedirecionamentoRecusado { status: 302 });
}
