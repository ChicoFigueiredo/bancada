//! O painel de conexões de mentira: imita a ESTRUTURA do Meu Pluggy (observada à mão no site
//! real) que o roteiro escrito `meu-pluggy-atualizar` usa, num servidor HTTP minúsculo em
//! `127.0.0.1`. **Nada de site real, nada de dado real.**
//!
//! - `GET /connections`: a lista, `#__next > div > main > section > div(2) > a(i)`, cada link
//!   com o texto `<Banco> Hoje Ver detalhes` (três conexões fictícias).
//! - `GET /connections/<id>`: a tela de detalhes. Lado a lado ficam "Atualizar" (1º botão) e
//!   "Excluir" (2º, o vizinho perigoso). A primeira conta só aparece depois de um atraso
//!   (`Carregando...` antes), como o site real.
//! - "Atualizar" (JS) avisa `POST /api/atualizou/<id>` e injeta um `<iframe>` do widget
//!   (`zoid-pluggy-connect-widget-…-visible`), que o próprio JS remove depois de
//!   [`Config::duracao_widget`].
//! - "Excluir" (JS) faz `POST /api/excluiu/<id>`; isso e qualquer acesso a
//!   `/connections/<id>/excluir` contam como acesso proibido: um roteiro que respeita a lista
//!   de bloqueio deixa [`PortalDeConexoes::exclusoes`] em zero.
//! - Com [`Config::sessao_expirada`], as duas telas devolvem só um "Entrar" (sem lista): o
//!   roteiro tem de pausar para o usuário.
//!
//! O módulo está sempre compilado: o `testar` do roteiro escrito o usa em produção, contra
//! `127.0.0.1`, sem rede externa.

use crate::site_falso::{ler_pedido, responder, travar};
use std::collections::BTreeMap;
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// O nome lógico do painel: o roteiro de exemplo declara este host no manifesto; o servidor
/// serve em `127.0.0.1:porta`.
pub const HOST_LOGICO_DO_PAINEL: &str = "meu.pluggy.ai";

/// As conexões fictícias: `(id, nome do banco)`. Os ids não significam nada.
const CONEXOES: [(&str, &str); 3] = [
    ("c0n3x40-0001", "Banco Um"),
    ("c0n3x40-0002", "Banco Dois"),
    ("c0n3x40-0003", "Banco Tres"),
];

/// A classe do iframe do widget (o prefixo é o que o roteiro procura; o resto imita o site).
const CLASSE_DO_WIDGET: &str = "zoid-pluggy-connect-widget-abc123-visible";
/// O `name` do iframe do widget (Base64 de "ficticio").
const NOME_DO_WIDGET: &str = "__zoid__pluggy_connect_widget__ZmljdGljaW8=";

/// Como o painel se comporta.
#[derive(Debug, Clone, Copy)]
pub struct Config {
    /// Quanto tempo o iframe do widget fica na tela depois de "Atualizar".
    pub duracao_widget: Duration,
    /// Quanto tempo a tela de detalhes mostra "Carregando..." antes de a primeira conta aparecer.
    pub atraso_das_contas: Duration,
    /// A sessão expirou: lista e detalhes devolvem só "Entrar".
    pub sessao_expirada: bool,
}

impl Default for Config {
    /// Widget de 2 s, contas depois de 1,5 s, sessão válida.
    fn default() -> Self {
        Self {
            duracao_widget: Duration::from_secs(2),
            atraso_das_contas: Duration::from_millis(1500),
            sessao_expirada: false,
        }
    }
}

impl Config {
    /// Troca a duração do widget.
    #[must_use]
    pub fn com_duracao_widget(mut self, duracao: Duration) -> Self {
        self.duracao_widget = duracao;
        self
    }

    /// Troca o atraso das contas.
    #[must_use]
    pub fn com_atraso_das_contas(mut self, atraso: Duration) -> Self {
        self.atraso_das_contas = atraso;
        self
    }

    /// Liga ou desliga a sessão expirada.
    #[must_use]
    pub fn com_sessao_expirada(mut self, expirada: bool) -> Self {
        self.sessao_expirada = expirada;
        self
    }
}

#[derive(Debug)]
struct Estado {
    config: Config,
    /// Atualizações por id de conexão.
    atualizacoes: Mutex<BTreeMap<String, usize>>,
    /// Acessos proibidos (`MÉTODO caminho`): excluir conexão.
    proibidos: Mutex<Vec<String>>,
    parar: AtomicBool,
}

/// O painel de conexões de mentira. Encerra o servidor no `Drop`.
#[derive(Debug)]
pub struct PortalDeConexoes {
    porta: u16,
    estado: Arc<Estado>,
    fio: Option<JoinHandle<()>>,
}

impl PortalDeConexoes {
    /// Sobe o painel numa porta livre de `127.0.0.1`.
    ///
    /// # Errors
    /// Se não conseguir abrir a porta.
    pub fn iniciar(config: Config) -> std::io::Result<Self> {
        let escuta = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))?;
        let porta = escuta.local_addr()?.port();
        let estado = Arc::new(Estado {
            config,
            atualizacoes: Mutex::new(BTreeMap::new()),
            proibidos: Mutex::new(Vec::new()),
            parar: AtomicBool::new(false),
        });
        let e = Arc::clone(&estado);
        let fio = std::thread::spawn(move || {
            for conexao in escuta.incoming() {
                if e.parar.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(c) = conexao else { continue };
                let e2 = Arc::clone(&e);
                std::thread::spawn(move || {
                    let _ = atender(c, &e2);
                });
            }
        });
        Ok(Self {
            porta,
            estado,
            fio: Some(fio),
        })
    }

    /// A URL base, sem barra no fim (`http://127.0.0.1:porta`).
    #[must_use]
    pub fn url_base(&self) -> String {
        format!("http://127.0.0.1:{}", self.porta)
    }

    /// A porta.
    #[must_use]
    pub fn porta(&self) -> u16 {
        self.porta
    }

    /// O nome lógico do painel ([`HOST_LOGICO_DO_PAINEL`]).
    #[must_use]
    pub fn host_logico(&self) -> &'static str {
        HOST_LOGICO_DO_PAINEL
    }

    /// Os ids das conexões fictícias, na ordem da lista.
    #[must_use]
    pub fn ids(&self) -> Vec<String> {
        CONEXOES.iter().map(|(id, _)| (*id).to_owned()).collect()
    }

    /// O nome do banco da conexão `id`.
    #[must_use]
    pub fn nome_de(&self, id: &str) -> Option<&'static str> {
        CONEXOES.iter().find(|(i, _)| *i == id).map(|(_, n)| *n)
    }

    /// Quantas vezes a conexão `id` foi atualizada (clique em "Atualizar").
    #[must_use]
    pub fn atualizacoes_de(&self, id: &str) -> usize {
        travar(&self.estado.atualizacoes)
            .get(id)
            .copied()
            .unwrap_or(0)
    }

    /// O total de atualizações, de todas as conexões.
    #[must_use]
    pub fn total_de_atualizacoes(&self) -> usize {
        travar(&self.estado.atualizacoes).values().sum()
    }

    /// Quantas vezes se alcançou "Excluir" (o clique ou a rota). Tem de ficar em zero.
    #[must_use]
    pub fn exclusoes(&self) -> usize {
        travar(&self.estado.proibidos).len()
    }

    /// Os acessos proibidos (`MÉTODO caminho`).
    #[must_use]
    pub fn acessos_proibidos(&self) -> Vec<String> {
        travar(&self.estado.proibidos).clone()
    }
}

impl Drop for PortalDeConexoes {
    fn drop(&mut self) {
        self.estado.parar.store(true, Ordering::SeqCst);
        // acorda o accept
        let _ = TcpStream::connect(SocketAddr::from(([127, 0, 0, 1], self.porta)));
        if let Some(f) = self.fio.take() {
            let _ = f.join();
        }
    }
}

/// A moldura de toda página: o conteúdo vai dentro de `#__next > div` (como o app Next do site
/// real); o script, depois dele, fica fora desses seletores.
fn pagina(titulo: &str, conteudo: &str, script: &str) -> Vec<u8> {
    format!(
        "<!doctype html><html lang=\"pt-BR\"><head><meta charset=\"utf-8\"><title>{titulo}</title>\
         <style>body{{font-family:sans-serif;margin:0}}main{{max-width:720px;margin:2em auto}}\
         a{{display:block;padding:10px;border:1px solid #999;margin:6px 0}}\
         button{{font-size:16px;padding:6px 12px;margin:4px}}</style></head><body>\
         <div id=\"__next\"><div>{conteudo}</div></div>{script}</body></html>"
    )
    .into_bytes()
}

fn pagina_da_lista() -> Vec<u8> {
    let mut links = String::new();
    for (id, nome) in CONEXOES {
        links.push_str(&format!(
            "<a href=\"/connections/{id}\">{nome} Hoje Ver detalhes</a>"
        ));
    }
    pagina(
        "Conexões",
        &format!(
            "<main><section><div><h1>Minhas conexões</h1></div><div>{links}</div></section></main>"
        ),
        "",
    )
}

fn pagina_de_entrar() -> Vec<u8> {
    pagina(
        "Entrar",
        "<main><section><div><h1>Meu Pluggy fictício</h1></div><div>\
         <p>Sua sessão expirou.</p><button type=\"button\">Entrar</button></div></section></main>",
        "",
    )
}

fn pagina_de_detalhes(id: &str, nome: &str, config: &Config) -> Vec<u8> {
    let atraso = config.atraso_das_contas.as_millis();
    let duracao = config.duracao_widget.as_millis();
    let conteudo = format!(
        "<main>\
         <div><div><div><h1>{nome}</h1></div>\
         <div><button type=\"button\" data-acao=\"atualizar\">Atualizar</button>\
         <button type=\"button\" data-acao=\"remover\">Excluir</button></div></div></div>\
         <section><div><h2>Resumo</h2></div><div><p>Conexão fictícia.</p></div></section>\
         <section><div><h2>Contas</h2></div><div id=\"contas\"><p>Carregando...</p></div></section>\
         </main>"
    );
    // O iframe do widget fica FORA do `#__next`, como o portal do zoid; some depois de `duracao`.
    let script = format!(
        "<script>(function(){{\
         var id='{id}';\
         setTimeout(function(){{document.getElementById('contas').innerHTML=\
         '<div><button type=\"button\">Conta corrente fictícia</button></div>';}},{atraso});\
         document.querySelector('[data-acao=atualizar]').addEventListener('click',function(){{\
         fetch('/api/atualizou/'+id,{{method:'POST'}});\
         var f=document.createElement('iframe');\
         f.className='{CLASSE_DO_WIDGET}';f.name='{NOME_DO_WIDGET}';f.src='/widget';\
         f.setAttribute('style','position:fixed;inset:0;width:100%;height:100%;border:0');\
         document.body.appendChild(f);\
         setTimeout(function(){{f.remove();}},{duracao});}});\
         document.querySelector('[data-acao=remover]').addEventListener('click',function(){{\
         fetch('/api/excluiu/'+id,{{method:'POST'}});}});\
         }})();</script>"
    );
    pagina(nome, &conteudo, &script)
}

fn pagina_do_widget() -> Vec<u8> {
    "<!doctype html><html lang=\"pt-BR\"><head><meta charset=\"utf-8\"><title>Widget</title></head>\
     <body style=\"font-family:sans-serif;background:#fff\"><h2>Conectando a sua conta</h2>\
     <p>Atualizando</p></body></html>"
        .as_bytes()
        .to_vec()
}

fn atender(mut c: TcpStream, e: &Estado) -> std::io::Result<()> {
    let Some(p) = ler_pedido(&mut c)? else {
        return Ok(());
    };
    let rota = p.caminho.split('?').next().unwrap_or("/").to_owned();
    let metodo = p.metodo.as_str();
    let html_ok = |c: &mut TcpStream, corpo: &[u8]| {
        responder(c, "200 OK", "", "text/html; charset=utf-8", corpo)
    };
    let nao_achou =
        |c: &mut TcpStream| responder(c, "404 Not Found", "", "text/plain", b"nao encontrado");
    let sem_corpo = |c: &mut TcpStream| {
        let r = responder(c, "204 No Content", "", "text/plain", b"");
        let _ = c.shutdown(Shutdown::Both);
        r
    };
    // Excluir, por qualquer caminho: acesso proibido.
    if rota.starts_with("/connections/") && rota.ends_with("/excluir") {
        travar(&e.proibidos).push(format!("{metodo} {rota}"));
        return html_ok(&mut c, "<p>Operação realizada.</p>".as_bytes());
    }
    match (metodo, rota.as_str()) {
        ("GET", "/") => responder(
            &mut c,
            "303 See Other",
            "Location: /connections\r\n",
            "text/plain",
            b"",
        ),
        ("GET", "/connections") => {
            if e.config.sessao_expirada {
                html_ok(&mut c, &pagina_de_entrar())
            } else {
                html_ok(&mut c, &pagina_da_lista())
            }
        }
        ("GET", "/widget") => html_ok(&mut c, &pagina_do_widget()),
        ("GET", r) if r.starts_with("/connections/") => {
            let id = &r["/connections/".len()..];
            match CONEXOES.iter().find(|(i, _)| *i == id) {
                Some((id, nome)) => {
                    if e.config.sessao_expirada {
                        html_ok(&mut c, &pagina_de_entrar())
                    } else {
                        html_ok(&mut c, &pagina_de_detalhes(id, nome, &e.config))
                    }
                }
                None => nao_achou(&mut c),
            }
        }
        ("POST", r) if r.starts_with("/api/atualizou/") => {
            let id = &r["/api/atualizou/".len()..];
            if CONEXOES.iter().any(|(i, _)| *i == id) {
                *travar(&e.atualizacoes).entry(id.to_owned()).or_insert(0) += 1;
                sem_corpo(&mut c)
            } else {
                nao_achou(&mut c)
            }
        }
        (m, r) if r.starts_with("/api/excluiu/") => {
            travar(&e.proibidos).push(format!("{m} {r}"));
            sem_corpo(&mut c)
        }
        _ => nao_achou(&mut c),
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::io::{Read, Write};

    fn pedir(portal: &PortalDeConexoes, metodo: &str, caminho: &str) -> (u16, String) {
        let mut c = TcpStream::connect(("127.0.0.1", portal.porta())).unwrap();
        c.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        write!(
            c,
            "{metodo} {caminho} HTTP/1.1\r\nHost: {HOST_LOGICO_DO_PAINEL}\r\n\
             Content-Length: 0\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        let mut bruto = Vec::new();
        c.read_to_end(&mut bruto).unwrap();
        let texto = String::from_utf8_lossy(&bruto).into_owned();
        let status = texto.split(' ').nth(1).unwrap().parse().unwrap();
        let corpo = texto
            .split_once("\r\n\r\n")
            .map_or("", |(_, b)| b)
            .to_owned();
        (status, corpo)
    }

    fn painel(config: Config) -> PortalDeConexoes {
        PortalDeConexoes::iniciar(config).unwrap()
    }

    #[test]
    fn a_lista_tem_tres_conexoes_na_estrutura_do_roteiro() {
        let p = painel(Config::default());
        assert_eq!(p.ids().len(), 3);
        let (status, html) = pedir(&p, "GET", "/connections");
        assert_eq!(status, 200);
        // #__next > div > main > section > div(1: título) + div(2: links)
        assert!(html.contains("<div id=\"__next\"><div><main><section><div><h1>"));
        assert!(html.contains("</h1></div><div><a href=\"/connections/"));
        for (id, nome) in CONEXOES {
            assert!(
                html.contains(&format!(
                    "<a href=\"/connections/{id}\">{nome} Hoje Ver detalhes</a>"
                )),
                "{html}"
            );
        }
        assert_eq!(html.matches("Ver detalhes</a>").count(), 3);
        assert_eq!(p.host_logico(), "meu.pluggy.ai");
        assert!(p.url_base().starts_with("http://127.0.0.1:"));
    }

    #[test]
    fn os_detalhes_tem_atualizar_e_excluir_lado_a_lado_e_contas_so_depois() {
        let p = painel(Config::default());
        let id = &p.ids()[0];
        let (status, html) = pedir(&p, "GET", &format!("/connections/{id}"));
        assert_eq!(status, 200);
        // main > div(1) > div(1) > div(2) > button(1) Atualizar, button(2) Excluir
        assert!(html.contains("<main><div><div><div><h1>Banco Um</h1></div><div><button"));
        let a = html.find(">Atualizar</button>").unwrap();
        let x = html.find(">Excluir</button>").unwrap();
        assert!(a < x, "Atualizar vem antes de Excluir");
        // main > section(2) > div(2): "Carregando..." no HTML; a conta só entra por JS (1,5 s)
        assert!(html.contains("<section><div><h2>Contas</h2></div><div id=\"contas\"><p>Carregando...</p></div></section>"));
        assert!(html.contains("},1500);"), "o atraso das contas é de 1,5 s");
        assert!(html.contains("Conta corrente fictícia"));
        // o clique em Atualizar avisa o servidor e injeta o iframe do widget
        assert!(html.contains("fetch('/api/atualizou/'+id"));
        assert!(html.contains("zoid-pluggy-connect-widget-abc123-visible"));
        assert!(html.contains("__zoid__pluggy_connect_widget__ZmljdGljaW8="));
        assert!(html.contains("f.src='/widget'"));
        assert!(html.contains("},2000);"), "o widget some em 2 s por padrão");
    }

    #[test]
    fn a_duracao_do_widget_e_o_atraso_das_contas_sao_configuraveis() {
        let p = painel(
            Config::default()
                .com_duracao_widget(Duration::from_millis(700))
                .com_atraso_das_contas(Duration::from_millis(300)),
        );
        let (_, html) = pedir(&p, "GET", &format!("/connections/{}", p.ids()[1]));
        assert!(html.contains("},700);"));
        assert!(html.contains("},300);"));
    }

    #[test]
    fn o_widget_e_uma_pagina_simples() {
        let p = painel(Config::default());
        let (status, html) = pedir(&p, "GET", "/widget");
        assert_eq!(status, 200);
        assert!(html.contains("Conectando a sua conta"));
        assert!(html.contains("Atualizando"));
    }

    #[test]
    fn conexao_desconhecida_e_404() {
        let p = painel(Config::default());
        assert_eq!(pedir(&p, "GET", "/connections/nada").0, 404);
        assert_eq!(pedir(&p, "POST", "/api/atualizou/nada").0, 404);
        assert_eq!(p.total_de_atualizacoes(), 0);
    }

    #[test]
    fn as_atualizacoes_sao_contadas_por_conexao() {
        let p = painel(Config::default());
        let ids = p.ids();
        assert_eq!(p.total_de_atualizacoes(), 0);
        assert_eq!(
            pedir(&p, "POST", &format!("/api/atualizou/{}", ids[0])).0,
            204
        );
        assert_eq!(
            pedir(&p, "POST", &format!("/api/atualizou/{}", ids[2])).0,
            204
        );
        assert_eq!(
            pedir(&p, "POST", &format!("/api/atualizou/{}", ids[2])).0,
            204
        );
        assert_eq!(p.atualizacoes_de(&ids[0]), 1);
        assert_eq!(p.atualizacoes_de(&ids[1]), 0);
        assert_eq!(p.atualizacoes_de(&ids[2]), 2);
        assert_eq!(p.total_de_atualizacoes(), 3);
        assert_eq!(p.exclusoes(), 0);
        assert_eq!(p.nome_de(&ids[1]), Some("Banco Dois"));
        assert_eq!(p.nome_de("x"), None);
    }

    #[test]
    fn excluir_conta_como_acesso_proibido() {
        let p = painel(Config::default());
        let id = &p.ids()[0];
        assert_eq!(p.exclusoes(), 0);
        pedir(&p, "POST", &format!("/api/excluiu/{id}"));
        assert_eq!(p.exclusoes(), 1);
        pedir(&p, "GET", &format!("/connections/{id}/excluir"));
        pedir(&p, "POST", &format!("/connections/{id}/excluir"));
        assert_eq!(p.exclusoes(), 3);
        let proibidos = p.acessos_proibidos();
        assert!(
            proibidos
                .iter()
                .any(|a| a.starts_with("POST /api/excluiu/"))
        );
        assert!(proibidos.iter().any(|a| a.starts_with("GET /connections/")));
        // nada disso conta como atualização
        assert_eq!(p.total_de_atualizacoes(), 0);
    }

    #[test]
    fn sessao_expirada_so_mostra_entrar() {
        let p = painel(Config::default().com_sessao_expirada(true));
        for caminho in [
            "/connections".to_owned(),
            format!("/connections/{}", p.ids()[0]),
        ] {
            let (status, html) = pedir(&p, "GET", &caminho);
            assert_eq!(status, 200);
            assert!(html.contains("Entrar"), "{html}");
            assert!(!html.contains("Ver detalhes"), "{html}");
            assert!(!html.contains(">Atualizar<"), "{html}");
        }
    }

    #[test]
    fn a_raiz_leva_a_lista() {
        let p = painel(Config::default());
        let (status, _) = pedir(&p, "GET", "/");
        assert_eq!(status, 303);
    }
}
