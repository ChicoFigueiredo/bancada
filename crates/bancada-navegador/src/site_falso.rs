//! O site de mentira: um servidor HTTP minúsculo em `127.0.0.1`, numa thread, para os testes
//! e a demonstração dos roteiros. **Nada de site real.** Páginas: login (CPF e senha), MFA
//! simulado, painel (com o botão Pagar, um link disfarçado e um formulário de transferência),
//! extratos (tabela) e o download de um PDF fictício.
//!
//! O servidor conta quantas vezes cada rota perigosa foi alcançada: um teste de bloqueio
//! afirma que o contador é zero.
//!
//! Além do banco de mentira, [`PortalDeContracheque`] é um portal de contracheque também de
//! mentira (login por CPF e senha, segunda etapa por e-mail, token ou CAPTCHA, lista de meses
//! e download de PDF fictício), para provar o roteiro escrito sem tocar site real. O módulo
//! está sempre compilado: o comando de teste do roteiro escrito o usa em produção, contra
//! `127.0.0.1`, sem rede externa.

use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime};

/// O CPF que o site aceita (só dígitos; o formulário tira a pontuação).
pub const CPF_DO_SITE: &str = "12345678909";
/// A senha fictícia que o site aceita.
pub const SENHA_DO_SITE: &str = "SenhaFicticia#4821";
/// O código de MFA do site.
pub const CODIGO_DE_MFA: &str = "123456";
/// O conteúdo do PDF fictício (outubro).
pub const PDF_FICTICIO: &[u8] =
    b"%PDF-1.4\n1 0 obj<<>>endobj\ntrailer<<>>\n% extrato ficticio de outubro\n%%EOF\n";

/// Como o site se comporta.
#[derive(Debug, Clone, Copy, Default)]
pub struct Configuracao {
    /// Pede o código depois do login.
    pub com_mfa: bool,
    /// Mostra um CAPTCHA antes do formulário de login.
    pub com_captcha: bool,
}

/// O que o site viu.
#[derive(Debug, Default)]
pub struct Contadores {
    /// Rotas de pagamento, transferência etc. alcançadas. Tem de ficar em zero.
    pub perigosas: AtomicUsize,
    /// Downloads servidos.
    pub downloads: AtomicUsize,
    /// Logins aceitos.
    pub logins: AtomicUsize,
    /// Pedidos de login recusados.
    pub logins_recusados: AtomicUsize,
}

#[derive(Debug)]
struct Estado {
    /// A porta do servidor (as páginas hostis a citam).
    porta: u16,
    configuracao: Configuracao,
    contadores: Contadores,
    /// O layout mudou: o botão de baixar ganha outro id (o "site alterado").
    layout_novo: AtomicBool,
    /// O CAPTCHA foi resolvido (o usuário "marcou a caixa").
    captcha_ok: AtomicBool,
    parar: AtomicBool,
}

/// O servidor.
#[derive(Debug)]
pub struct SiteFalso {
    porta: u16,
    estado: Arc<Estado>,
    fio: Option<JoinHandle<()>>,
}

impl SiteFalso {
    /// Sobe o servidor numa porta livre de `127.0.0.1`.
    ///
    /// # Errors
    /// Se não conseguir abrir a porta.
    pub fn iniciar(configuracao: Configuracao) -> std::io::Result<Self> {
        let escuta = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))?;
        let porta = escuta.local_addr()?.port();
        let estado = Arc::new(Estado {
            porta,
            configuracao,
            contadores: Contadores::default(),
            layout_novo: AtomicBool::new(false),
            captcha_ok: AtomicBool::new(false),
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

    /// A URL base (`http://127.0.0.1:porta`).
    #[must_use]
    pub fn url(&self, caminho: &str) -> String {
        format!("http://127.0.0.1:{}{caminho}", self.porta)
    }

    /// A porta.
    #[must_use]
    pub fn porta(&self) -> u16 {
        self.porta
    }

    /// Quantas rotas perigosas foram alcançadas.
    #[must_use]
    pub fn perigosas(&self) -> usize {
        self.estado.contadores.perigosas.load(Ordering::SeqCst)
    }

    /// Quantos downloads foram servidos.
    #[must_use]
    pub fn downloads(&self) -> usize {
        self.estado.contadores.downloads.load(Ordering::SeqCst)
    }

    /// Quantos logins foram aceitos.
    #[must_use]
    pub fn logins(&self) -> usize {
        self.estado.contadores.logins.load(Ordering::SeqCst)
    }

    /// Quantos logins foram recusados.
    #[must_use]
    pub fn logins_recusados(&self) -> usize {
        self.estado
            .contadores
            .logins_recusados
            .load(Ordering::SeqCst)
    }

    /// Muda o layout: o botão de baixar passa a ter outro id (o site foi alterado).
    pub fn mudar_layout(&self) {
        self.estado.layout_novo.store(true, Ordering::SeqCst);
    }
}

impl Drop for SiteFalso {
    fn drop(&mut self) {
        self.estado.parar.store(true, Ordering::SeqCst);
        // acorda o accept
        let _ = TcpStream::connect(SocketAddr::from(([127, 0, 0, 1], self.porta)));
        if let Some(f) = self.fio.take() {
            let _ = f.join();
        }
    }
}

struct Pedido {
    metodo: String,
    caminho: String,
    cookies: String,
    corpo: String,
}

fn ler_pedido(c: &mut TcpStream) -> std::io::Result<Option<Pedido>> {
    c.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut buf = Vec::new();
    let mut parte = [0_u8; 4096];
    let fim_do_cabecalho = loop {
        let n = c.read(&mut parte)?;
        if n == 0 {
            return Ok(None);
        }
        buf.extend_from_slice(&parte[..n]);
        if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break p + 4;
        }
        if buf.len() > 64 * 1024 {
            return Ok(None);
        }
    };
    let cabecalho = String::from_utf8_lossy(&buf[..fim_do_cabecalho]).into_owned();
    let mut linhas = cabecalho.lines();
    let primeira = linhas.next().unwrap_or("");
    let mut p = primeira.split(' ');
    let metodo = p.next().unwrap_or("").to_owned();
    let caminho = p.next().unwrap_or("/").to_owned();
    let mut tamanho = 0_usize;
    let mut cookies = String::new();
    for l in linhas {
        if let Some((k, v)) = l.split_once(':') {
            match k.trim().to_ascii_lowercase().as_str() {
                "content-length" => tamanho = v.trim().parse().unwrap_or(0),
                "cookie" => cookies.push_str(v.trim()),
                _ => {}
            }
        }
    }
    while buf.len() < fim_do_cabecalho + tamanho {
        let n = c.read(&mut parte)?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&parte[..n]);
    }
    let corpo = String::from_utf8_lossy(&buf[fim_do_cabecalho..]).into_owned();
    Ok(Some(Pedido {
        metodo,
        caminho,
        cookies,
        corpo,
    }))
}

fn campo_do_formulario(corpo: &str, nome: &str) -> String {
    corpo
        .split('&')
        .filter_map(|p| p.split_once('='))
        .find(|(k, _)| *k == nome)
        .map(|(_, v)| decodificar(v))
        .unwrap_or_default()
}

fn decodificar(v: &str) -> String {
    let b = v.as_bytes();
    let mut saida = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => saida.push(b' '),
            b'%' if i + 2 < b.len() => {
                let h = std::str::from_utf8(&b[i + 1..i + 3])
                    .ok()
                    .and_then(|h| u8::from_str_radix(h, 16).ok());
                if let Some(x) = h {
                    saida.push(x);
                    i += 2;
                } else {
                    saida.push(b'%');
                }
            }
            x => saida.push(x),
        }
        i += 1;
    }
    String::from_utf8_lossy(&saida).into_owned()
}

fn tem(cookies: &str, nome: &str) -> bool {
    cookies.split(';').any(|c| c.trim() == format!("{nome}=ok"))
}

fn html(titulo: &str, corpo: &str) -> Vec<u8> {
    format!(
        "<!doctype html><html lang=\"pt-BR\"><head><meta charset=\"utf-8\"><title>{titulo}</title>\
         <style>body{{font-family:sans-serif;max-width:640px;margin:2em auto}}\
         input,button,a.botao{{font-size:16px;padding:6px 10px;margin:4px 0}}table{{border-collapse:collapse}}\
         td,th{{border:1px solid #999;padding:4px 10px}}</style></head><body>\
         <h1>Banco Ficticio</h1>{corpo}</body></html>"
    )
    .into_bytes()
}

fn responder(
    c: &mut TcpStream,
    status: &str,
    extra: &str,
    tipo: &str,
    corpo: &[u8],
) -> std::io::Result<()> {
    write!(
        c,
        "HTTP/1.1 {status}\r\nContent-Type: {tipo}\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n",
        corpo.len()
    )?;
    c.write_all(corpo)?;
    c.flush()?;
    let _ = c.shutdown(Shutdown::Write);
    Ok(())
}

fn redirecionar(c: &mut TcpStream, para: &str, cookie: Option<&str>) -> std::io::Result<()> {
    let extra = format!(
        "Location: {para}\r\n{}",
        cookie
            .map(|k| format!("Set-Cookie: {k}; Path=/\r\n"))
            .unwrap_or_default()
    );
    responder(c, "303 See Other", &extra, "text/plain", b"")
}

fn atender(mut c: TcpStream, e: &Estado) -> std::io::Result<()> {
    let Some(p) = ler_pedido(&mut c)? else {
        return Ok(());
    };
    let rota = p.caminho.split('?').next().unwrap_or("/").to_owned();
    let html_ok = |c: &mut TcpStream, t: &str, corpo: &str| {
        responder(c, "200 OK", "", "text/html; charset=utf-8", &html(t, corpo))
    };
    let logado = tem(&p.cookies, "sessao") && (!e.configuracao.com_mfa || tem(&p.cookies, "mfa"));
    match (p.metodo.as_str(), rota.as_str()) {
        ("GET", "/") => redirecionar(&mut c, "/login", None),
        ("GET", "/login") => {
            let erro = if p.caminho.contains("erro=1") {
                "<p id=\"erro\">Dados inválidos.</p>"
            } else {
                ""
            };
            let captcha = if e.configuracao.com_captcha && !e.captcha_ok.load(Ordering::SeqCst) {
                "<div class=\"g-recaptcha\" data-sitekey=\"falsa\"><label><input type=\"checkbox\" id=\"humano\" \
                 onclick=\"fetch('/captcha-ok',{method:'POST'})\"> Não sou um robô</label></div>"
            } else {
                ""
            };
            html_ok(
                &mut c,
                "Entrar",
                &format!(
                    "{erro}<p>Acesse sua conta</p>{captcha}<form id=\"f\" method=\"post\" action=\"/login\">\
                 <label>CPF <input id=\"cpf\" name=\"cpf\" type=\"text\" autocomplete=\"username\"></label><br>\
                 <label>Senha <input id=\"senha\" name=\"senha\" type=\"password\" autocomplete=\"current-password\"></label><br>\
                 <button id=\"entrar\" type=\"submit\">Entrar</button></form>"
                ),
            )
        }
        ("POST", "/captcha-ok") => {
            e.captcha_ok.store(true, Ordering::SeqCst);
            responder(&mut c, "204 No Content", "", "text/plain", b"")
        }
        ("POST", "/login") => {
            let cpf: String = campo_do_formulario(&p.corpo, "cpf")
                .chars()
                .filter(char::is_ascii_digit)
                .collect();
            let senha = campo_do_formulario(&p.corpo, "senha");
            if cpf == CPF_DO_SITE && senha == SENHA_DO_SITE {
                e.contadores.logins.fetch_add(1, Ordering::SeqCst);
                if e.configuracao.com_mfa {
                    redirecionar(&mut c, "/mfa", Some("sessao=ok"))
                } else {
                    redirecionar(&mut c, "/painel", Some("sessao=ok"))
                }
            } else {
                e.contadores.logins_recusados.fetch_add(1, Ordering::SeqCst);
                redirecionar(&mut c, "/login?erro=1", None)
            }
        }
        ("GET", "/mfa") => html_ok(
            &mut c,
            "Verificação",
            "<p>Digite o código de verificação enviado por SMS.</p>\
             <form id=\"fm\" method=\"post\" action=\"/mfa\"><input id=\"codigo\" name=\"codigo\" \
             autocomplete=\"one-time-code\"><button id=\"validar\" type=\"submit\">Validar</button></form>",
        ),
        ("POST", "/mfa") => {
            if campo_do_formulario(&p.corpo, "codigo") == CODIGO_DE_MFA {
                redirecionar(&mut c, "/painel", Some("mfa=ok"))
            } else {
                redirecionar(&mut c, "/mfa", None)
            }
        }
        ("GET", "/painel") if logado => html_ok(
            &mut c,
            "Painel",
            "<p>Bem-vindo ao painel.</p><nav><a id=\"extratos-link\" href=\"/extratos\">Extratos</a></nav>\
             <form method=\"post\" action=\"/pagar\"><button id=\"pagar\" type=\"submit\">Pagar</button></form>\
             <a id=\"seguir\" class=\"botao\" href=\"/pagamento/confirmar\">Continuar</a>\
             <form method=\"post\" action=\"/transferir\"><button id=\"enviar\" type=\"submit\">Enviar</button></form>\
             <button id=\"pix\" onclick=\"location.href='/pix'\">PIX</button>",
        ),
        ("GET", "/extratos") if logado => {
            let id = if e.layout_novo.load(Ordering::SeqCst) {
                "baixar-extrato"
            } else {
                "baixar"
            };
            html_ok(
                &mut c,
                "Extratos",
                &format!(
                    "<table id=\"extratos\"><tr><th>Mês</th><th>Valor</th></tr>\
                 <tr><td>Outubro</td><td>R$ 1.234,56</td></tr><tr><td>Setembro</td><td>R$ 987,65</td></tr></table>\
                 <p><a id=\"{id}\" href=\"/extrato-2026-10.pdf\" download>Baixar extrato de outubro</a></p>"
                ),
            )
        }
        ("GET", "/extrato-2026-10.pdf") if logado => {
            e.contadores.downloads.fetch_add(1, Ordering::SeqCst);
            responder(
                &mut c,
                "200 OK",
                "Content-Disposition: attachment; filename=\"extrato-2026-10.pdf\"\r\n",
                "application/pdf",
                PDF_FICTICIO,
            )
        }
        // Páginas hostis dos testes de segurança (RO-01, RO-02): nada aqui é um site real.
        ("GET", "/hostil") => html_ok(
            &mut c,
            "Entrar",
            // Mente para o SBF: troca a ponte da página por uma que jura estar no site do login.
            &format!(
                "<p>Acesse sua conta</p><form method=\"post\" action=\"/login\">\
                 <input id=\"senha\" name=\"senha\" type=\"password\"></form>\
                 <script>window.__sbfPonte={{pagina:()=>({{url:'http://127.0.0.1:{}/login',titulo:'Entrar',\
                 texto:'',elementos:[],formularios:[],tabelas:[],quadros:[]}}),\
                 descrever:()=>({{campos:[],eh_senha:true,eh_campo:true,tipo:'password'}}),\
                 visivel:()=>true,bloqueios:()=>[]}};</script>",
                e.porta
            ),
        ),
        ("GET", "/continuar") => html_ok(
            &mut c,
            "Continuar",
            // Um botão de texto inocente que, por script, paga e transfere.
            "<button id=\"continuar\" onclick=\"fetch('/pagar',{method:'POST'});location='/transferir'\">\
             Continuar</button><button id=\"baixar\" onclick=\"location='/extrato-2026-10.pdf'\">Baixar</button>",
        ),
        ("GET", "/coberto") => html_ok(
            &mut c,
            "Coberto",
            // O botão de verdade fica sob uma capa transparente que leva a outro lugar.
            "<button id=\"alvo\" style=\"position:absolute;left:40px;top:40px;width:200px;height:60px\" \
             onclick=\"document.title='alvo'\">Baixar extrato</button>\
             <div id=\"capa\" style=\"position:fixed;left:0;top:0;width:100%;height:100%\" \
             onclick=\"location='/navegou'\"></div>",
        ),
        ("GET", "/coberto-por-quadro") => html_ok(
            &mut c,
            "Coberto",
            "<button id=\"alvo\" style=\"position:absolute;left:40px;top:40px;width:200px;height:60px\" \
             onclick=\"document.title='alvo'\">Baixar extrato</button>\
             <iframe id=\"quadro\" srcdoc=\"<body onclick=&quot;top.location='/navegou'&quot; \
             style='margin:0;height:100vh'></body>\" style=\"position:fixed;left:0;top:0;width:100%;\
             height:100%;border:0\"></iframe>",
        ),
        ("GET", "/abre-janela") => html_ok(
            &mut c,
            "Janela",
            // Abre uma janela nova (que o Chrome trata como outro alvo) para o que o clique
            // inocente não pode fazer na aba vigiada.
            "<button id=\"abre\" onclick=\"window.open('/transferir')\">Continuar</button>",
        ),
        ("GET", "/comprovante") => html_ok(
            &mut c,
            "Comprovante",
            "<button id=\"ok\" onclick=\"document.title='clicou'\">Comprovante de pagamento</button>\
             <button id=\"mau\" onclick=\"document.title='mau'\">Comprovante de pagamento: pagar agora</button>",
        ),
        ("GET", "/navegou") => html_ok(&mut c, "Navegou", "<p>Fui levado para outro lugar.</p>"),
        ("GET", "/sombra") => html_ok(
            &mut c,
            "Sombra",
            // O botão "Pagar" mora num shadow DOM: o evento chega à janela com o alvo trocado
            // pelo hospedeiro, que não tem texto nenhum.
            "<div id=\"hospedeiro\" style=\"display:block;width:200px;height:60px\"></div>\
             <script>const r=document.getElementById('hospedeiro').attachShadow({mode:'open'});\
             r.innerHTML=\"<button style='width:200px;height:60px'>Pagar</button>\";\
             r.firstChild.onclick=()=>{location='/navegou'};</script>",
        ),
        ("GET", "/quadro-de-fora") => html_ok(
            &mut c,
            "Quadro",
            // Um iframe de OUTRO site (localhost x 127.0.0.1): o Chrome o roda em outro processo.
            &format!(
                "<p>Painel</p><iframe id=\"q\" src=\"http://localhost:{}/envia\"></iframe>",
                e.porta
            ),
        ),
        ("GET", "/envia") => html_ok(
            &mut c,
            "Envia",
            "<script>fetch('/pagar',{method:'POST'});</script>",
        ),
        (_, "/pagar" | "/pagamento/confirmar" | "/transferir" | "/pix") => {
            e.contadores.perigosas.fetch_add(1, Ordering::SeqCst);
            html_ok(&mut c, "Feito", "<p>Operação realizada.</p>")
        }
        (_, "/painel" | "/extratos" | "/extrato-2026-10.pdf") => {
            redirecionar(&mut c, "/login", None)
        }
        _ => responder(&mut c, "404 Not Found", "", "text/plain", b"nao encontrado"),
    }
}

// ---------------------------------------------------------------------------------------------
// Portal de contracheque (fictício)
// ---------------------------------------------------------------------------------------------

/// O nome lógico do portal: `portal.exemplo.test` não resolve em DNS nenhum; o portal serve em
/// `127.0.0.1:porta`, e o roteiro de exemplo é testado com este nome no manifesto.
pub const HOST_LOGICO_DO_PORTAL: &str = "portal.exemplo.test";

/// Segunda etapa de autenticação do portal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MfaDoPortal {
    /// Código de 6 dígitos "enviado por e-mail" (campo `#codigo`).
    #[default]
    Email,
    /// Código do "app autenticador" (campo `#token`): o roteiro tem de pausar para o usuário.
    /// O código aceito é o mesmo de [`PortalDeContracheque::codigo_gerado`].
    Token,
    /// Página com CAPTCHA ("Não sou um robô"): o roteiro tem de pausar. O usuário "resolve"
    /// marcando a caixa, ou o teste faz `POST /_teste/liberar`.
    Captcha,
}

/// Como o portal se comporta.
#[derive(Debug, Clone)]
pub struct Config {
    /// O CPF aceito (só dígitos; o formulário tira a pontuação).
    pub cpf: String,
    /// A senha aceita.
    pub senha: String,
    /// A segunda etapa.
    pub mfa: MfaDoPortal,
    /// Se `Some`, todo login certo gera este código (6 dígitos, por exemplo `"482913"`);
    /// se `None`, um código aleatório de 6 dígitos a cada login certo.
    pub codigo_fixo: Option<String>,
    /// Os meses publicados, como `AAAA-MM`. Meses fora dessa lista não existem no portal
    /// (o roteiro de exemplo trata o "mês ainda não publicado").
    pub meses: Vec<String>,
}

impl Config {
    /// Credenciais fictícias dadas pelo teste; MFA por e-mail, código aleatório e os meses
    /// `2026-10` e `2026-11`.
    #[must_use]
    pub fn nova(cpf: &str, senha: &str) -> Self {
        Self {
            cpf: cpf.chars().filter(char::is_ascii_digit).collect(),
            senha: senha.to_owned(),
            mfa: MfaDoPortal::Email,
            codigo_fixo: None,
            meses: vec!["2026-10".to_owned(), "2026-11".to_owned()],
        }
    }

    /// Troca a segunda etapa.
    #[must_use]
    pub fn com_mfa(mut self, mfa: MfaDoPortal) -> Self {
        self.mfa = mfa;
        self
    }

    /// Fixa o código gerado a cada login certo.
    #[must_use]
    pub fn com_codigo(mut self, codigo: &str) -> Self {
        self.codigo_fixo = Some(codigo.to_owned());
        self
    }

    /// Troca os meses publicados (`AAAA-MM`; o que não tiver esse formato é ignorado).
    #[must_use]
    pub fn com_meses(mut self, meses: &[&str]) -> Self {
        self.meses = meses.iter().map(|m| (*m).to_owned()).collect();
        self
    }
}

impl Default for Config {
    /// [`CPF_DO_SITE`] e [`SENHA_DO_SITE`].
    fn default() -> Self {
        Self::nova(CPF_DO_SITE, SENHA_DO_SITE)
    }
}

#[derive(Debug)]
struct EstadoDoPortal {
    config: Config,
    /// O último código gerado e quando (a `CaixaFalsa` dos testes simula o e-mail com isto).
    codigo: Mutex<Option<(String, SystemTime)>>,
    /// O usuário "resolveu" o token ou o CAPTCHA.
    liberado: AtomicBool,
    baixados: Mutex<Vec<String>>,
    proibidos: Mutex<Vec<String>>,
    logins: AtomicUsize,
    logins_recusados: AtomicUsize,
    codigos_recusados: AtomicUsize,
    parar: AtomicBool,
}

fn travar<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// O portal de contracheque de mentira. Encerra o servidor no `Drop`.
#[derive(Debug)]
pub struct PortalDeContracheque {
    porta: u16,
    estado: Arc<EstadoDoPortal>,
    fio: Option<JoinHandle<()>>,
}

/// `AAAA-MM` com mês de 01 a 12.
fn mes_valido(m: &str) -> bool {
    let b = m.as_bytes();
    b.len() == 7
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[4] == b'-'
        && matches!(
            &m[5..],
            "01" | "02" | "03" | "04" | "05" | "06" | "07" | "08" | "09" | "10" | "11" | "12"
        )
}

fn nome_do_mes(m: &str) -> String {
    let nome = match &m[5..] {
        "01" => "Janeiro",
        "02" => "Fevereiro",
        "03" => "Março",
        "04" => "Abril",
        "05" => "Maio",
        "06" => "Junho",
        "07" => "Julho",
        "08" => "Agosto",
        "09" => "Setembro",
        "10" => "Outubro",
        "11" => "Novembro",
        _ => "Dezembro",
    };
    format!("{nome}/{}", &m[..4])
}

/// Seis dígitos aleatórios.
fn codigo_aleatorio() -> String {
    let mut b = [0_u8; 4];
    // Sem fonte de aleatoriedade, cai num código fixo: é um site de mentira.
    if getrandom::fill(&mut b).is_err() {
        return "000000".to_owned();
    }
    format!("{:06}", u32::from_le_bytes(b) % 1_000_000)
}

impl PortalDeContracheque {
    /// Sobe o portal numa porta livre de `127.0.0.1`.
    ///
    /// # Errors
    /// Se não conseguir abrir a porta.
    pub fn iniciar(mut config: Config) -> std::io::Result<Self> {
        config.meses.retain(|m| mes_valido(m));
        config.cpf.retain(|c| c.is_ascii_digit());
        let escuta = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))?;
        let porta = escuta.local_addr()?.port();
        let estado = Arc::new(EstadoDoPortal {
            config,
            codigo: Mutex::new(None),
            liberado: AtomicBool::new(false),
            baixados: Mutex::new(Vec::new()),
            proibidos: Mutex::new(Vec::new()),
            logins: AtomicUsize::new(0),
            logins_recusados: AtomicUsize::new(0),
            codigos_recusados: AtomicUsize::new(0),
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
                    let _ = atender_portal(c, &e2);
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

    /// O nome lógico do portal ([`HOST_LOGICO_DO_PORTAL`]).
    #[must_use]
    pub fn host_logico(&self) -> &'static str {
        HOST_LOGICO_DO_PORTAL
    }

    /// O último código gerado por um login certo (o que a `CaixaFalsa` põe no "e-mail").
    #[must_use]
    pub fn codigo_gerado(&self) -> Option<String> {
        travar(&self.estado.codigo).as_ref().map(|(c, _)| c.clone())
    }

    /// Quando o último código foi gerado.
    #[must_use]
    pub fn instante_do_codigo(&self) -> Option<SystemTime> {
        travar(&self.estado.codigo).as_ref().map(|(_, t)| *t)
    }

    /// O mês (`AAAA-MM`) foi baixado pelo menos uma vez?
    #[must_use]
    pub fn baixou(&self, mes: &str) -> bool {
        travar(&self.estado.baixados).iter().any(|m| m == mes)
    }

    /// Quantos downloads foram servidos (de qualquer mês).
    #[must_use]
    pub fn downloads(&self) -> usize {
        travar(&self.estado.baixados).len()
    }

    /// Tudo que alcançou "Pagar boleto" ou "Alterar cadastro" (`MÉTODO caminho`). Um roteiro
    /// que respeita a lista de bloqueio deixa isto vazio.
    #[must_use]
    pub fn acessos_proibidos(&self) -> Vec<String> {
        travar(&self.estado.proibidos).clone()
    }

    /// Logins aceitos.
    #[must_use]
    pub fn logins(&self) -> usize {
        self.estado.logins.load(Ordering::SeqCst)
    }

    /// Logins recusados.
    #[must_use]
    pub fn logins_recusados(&self) -> usize {
        self.estado.logins_recusados.load(Ordering::SeqCst)
    }

    /// Códigos (ou tokens) recusados.
    #[must_use]
    pub fn codigos_recusados(&self) -> usize {
        self.estado.codigos_recusados.load(Ordering::SeqCst)
    }

    /// Simula o usuário resolvendo o token ou o CAPTCHA (o mesmo de `POST /_teste/liberar`).
    pub fn liberar(&self) {
        self.estado.liberado.store(true, Ordering::SeqCst);
    }
}

impl Drop for PortalDeContracheque {
    fn drop(&mut self) {
        self.estado.parar.store(true, Ordering::SeqCst);
        // acorda o accept
        let _ = TcpStream::connect(SocketAddr::from(([127, 0, 0, 1], self.porta)));
        if let Some(f) = self.fio.take() {
            let _ = f.join();
        }
    }
}

/// Um PDF minúsculo e FICTÍCIO (uma página, o texto "Contracheque FICTÍCIO AAAA-MM"),
/// determinístico por mês. As posições da tabela `xref` são calculadas.
#[must_use]
pub fn pdf_do_contracheque(mes: &str) -> Vec<u8> {
    // O Í sai como escape octal (`\315`, WinAnsi): o arquivo inteiro é ASCII.
    let conteudo = format!("BT /F1 18 Tf 72 720 Td (Contracheque FIC\\315TICIO {mes}) Tj ET");
    let objetos = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Contents 4 0 R \
         /Resources << /Font << /F1 5 0 R >> >> >>"
            .to_owned(),
        format!(
            "<< /Length {} >>\nstream\n{conteudo}\nendstream",
            conteudo.len()
        ),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
            .to_owned(),
    ];
    let mut pdf = String::from("%PDF-1.4\n");
    let mut posicoes = Vec::new();
    for (i, o) in objetos.iter().enumerate() {
        posicoes.push(pdf.len());
        pdf.push_str(&format!("{} 0 obj\n{o}\nendobj\n", i + 1));
    }
    let xref = pdf.len();
    pdf.push_str(&format!(
        "xref\n0 {}\n0000000000 65535 f \n",
        objetos.len() + 1
    ));
    for p in posicoes {
        pdf.push_str(&format!("{p:010} 00000 n \n"));
    }
    pdf.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objetos.len() + 1
    ));
    pdf.into_bytes()
}

fn html_portal(titulo: &str, corpo: &str) -> Vec<u8> {
    format!(
        "<!doctype html><html lang=\"pt-BR\"><head><meta charset=\"utf-8\"><title>{titulo}</title>\
         <style>body{{font-family:sans-serif;max-width:640px;margin:2em auto}}\
         input,button,a.botao{{font-size:16px;padding:6px 10px;margin:4px 0}}table{{border-collapse:collapse}}\
         td,th{{border:1px solid #999;padding:4px 10px}}</style></head><body>\
         <h1>Portal de Contracheque Fictício</h1>{corpo}</body></html>"
    )
    .into_bytes()
}

/// A página espera o usuário (token ou CAPTCHA) e segue sozinha quando ele "libera".
const ESPERA_DA_LIBERACAO: &str = "<script>setInterval(()=>fetch('/_teste/liberado')\
    .then(r=>r.text()).then(t=>{if(t==='1')location='/contracheques'}),500);</script>";

fn pagina_do_codigo(mfa: MfaDoPortal, erro: bool) -> String {
    let erro = if erro {
        "<p id=\"erro\">Código inválido. Tente de novo.</p>"
    } else {
        ""
    };
    match mfa {
        MfaDoPortal::Email => format!(
            "{erro}<p>Enviamos um código por e-mail. Digite-o abaixo para continuar.</p>\
             <form id=\"fm\" method=\"post\" action=\"/codigo\">\
             <input id=\"codigo\" name=\"codigo\" autocomplete=\"one-time-code\">\
             <button id=\"confirmar\" type=\"submit\">Confirmar</button></form>"
        ),
        MfaDoPortal::Token => format!(
            "{erro}<p>Digite o código do seu app autenticador.</p>\
             <form id=\"fm\" method=\"post\" action=\"/codigo\">\
             <input id=\"token\" name=\"token\" autocomplete=\"one-time-code\">\
             <button id=\"confirmar\" type=\"submit\">Confirmar</button></form>{ESPERA_DA_LIBERACAO}"
        ),
        MfaDoPortal::Captcha => format!(
            "<p>Antes de continuar, confirme o captcha.</p>\
             <div class=\"g-recaptcha\" data-sitekey=\"falsa\"><label><input type=\"checkbox\" \
             id=\"humano\" onclick=\"fetch('/_teste/liberar',{{method:'POST'}})\"> Não sou um robô</label></div>\
             {ESPERA_DA_LIBERACAO}"
        ),
    }
}

fn pagina_dos_contracheques(meses: &[String]) -> String {
    let mut linhas = String::new();
    // Mais recente primeiro.
    let mut ordenados: Vec<&String> = meses.iter().collect();
    ordenados.sort();
    ordenados.reverse();
    for m in ordenados {
        linhas.push_str(&format!(
            "<tr><td>{}</td><td><a class=\"baixar\" id=\"baixar-{m}\" href=\"/contracheque/{m}.pdf\">Baixar PDF</a></td></tr>",
            nome_do_mes(m)
        ));
    }
    format!(
        "<p>Seus contracheques.</p>\
         <table id=\"lista\"><tr><th>Competência</th><th>Arquivo</th></tr>{linhas}</table>\
         <form method=\"post\" action=\"/boleto/pagar\"><button id=\"pagar-boleto\" type=\"submit\">Pagar boleto</button></form>\
         <p><a id=\"alterar-cadastro\" href=\"/cadastro/alterar\">Alterar cadastro</a></p>"
    )
}

fn atender_portal(mut c: TcpStream, e: &EstadoDoPortal) -> std::io::Result<()> {
    let Some(p) = ler_pedido(&mut c)? else {
        return Ok(());
    };
    let rota = p.caminho.split('?').next().unwrap_or("/").to_owned();
    let html_ok = |c: &mut TcpStream, t: &str, corpo: &str| {
        responder(
            c,
            "200 OK",
            "",
            "text/html; charset=utf-8",
            &html_portal(t, corpo),
        )
    };
    let com_sessao = tem(&p.cookies, "sessao");
    let segunda_etapa = tem(&p.cookies, "mfa") || e.liberado.load(Ordering::SeqCst);
    let logado = com_sessao && segunda_etapa;
    match (p.metodo.as_str(), rota.as_str()) {
        ("GET", "/") => redirecionar(&mut c, "/entrar", None),
        ("GET", "/entrar") => {
            let erro = if p.caminho.contains("erro=1") {
                "<p id=\"erro\">CPF ou senha inválidos.</p>"
            } else {
                ""
            };
            html_ok(
                &mut c,
                "Entrar",
                &format!(
                    "{erro}<p>Acesse seu contracheque</p><form id=\"f\" method=\"post\" action=\"/entrar\">\
                     <label>CPF <input id=\"cpf\" name=\"cpf\" type=\"text\" autocomplete=\"username\"></label><br>\
                     <label>Senha <input id=\"senha\" name=\"senha\" type=\"password\" autocomplete=\"current-password\"></label><br>\
                     <button id=\"entrar\" type=\"submit\">Entrar</button></form>"
                ),
            )
        }
        ("POST", "/entrar") => {
            let cpf: String = campo_do_formulario(&p.corpo, "cpf")
                .chars()
                .filter(char::is_ascii_digit)
                .collect();
            let senha = campo_do_formulario(&p.corpo, "senha");
            if cpf == e.config.cpf && senha == e.config.senha {
                e.logins.fetch_add(1, Ordering::SeqCst);
                let codigo = e
                    .config
                    .codigo_fixo
                    .clone()
                    .unwrap_or_else(codigo_aleatorio);
                *travar(&e.codigo) = Some((codigo, SystemTime::now()));
                redirecionar(&mut c, "/codigo", Some("sessao=ok"))
            } else {
                e.logins_recusados.fetch_add(1, Ordering::SeqCst);
                redirecionar(&mut c, "/entrar?erro=1", None)
            }
        }
        ("GET", "/codigo") if com_sessao => {
            if segunda_etapa {
                redirecionar(&mut c, "/contracheques", None)
            } else {
                let erro = p.caminho.contains("erro=1");
                html_ok(&mut c, "Verificação", &pagina_do_codigo(e.config.mfa, erro))
            }
        }
        ("POST", "/codigo") if com_sessao => {
            let campo = match e.config.mfa {
                MfaDoPortal::Email => "codigo",
                MfaDoPortal::Token => "token",
                MfaDoPortal::Captcha => "",
            };
            let enviado = campo_do_formulario(&p.corpo, campo);
            let esperado = travar(&e.codigo).as_ref().map(|(c, _)| c.clone());
            if !campo.is_empty() && !enviado.is_empty() && esperado.as_deref() == Some(&enviado) {
                redirecionar(&mut c, "/contracheques", Some("mfa=ok"))
            } else {
                e.codigos_recusados.fetch_add(1, Ordering::SeqCst);
                redirecionar(&mut c, "/codigo?erro=1", None)
            }
        }
        ("POST", "/_teste/liberar") => {
            e.liberado.store(true, Ordering::SeqCst);
            responder(&mut c, "204 No Content", "", "text/plain", b"")
        }
        ("GET", "/_teste/liberado") => {
            let v: &[u8] = if e.liberado.load(Ordering::SeqCst) {
                b"1"
            } else {
                b"0"
            };
            responder(&mut c, "200 OK", "", "text/plain", v)
        }
        ("GET", "/contracheques") if logado => html_ok(
            &mut c,
            "Contracheques",
            &pagina_dos_contracheques(&e.config.meses),
        ),
        ("GET", r) if r.starts_with("/contracheque/") && r.ends_with(".pdf") && logado => {
            let mes = &r["/contracheque/".len()..r.len() - ".pdf".len()];
            if e.config.meses.iter().any(|m| m == mes) {
                travar(&e.baixados).push(mes.to_owned());
                responder(
                    &mut c,
                    "200 OK",
                    &format!(
                        "Content-Disposition: attachment; filename=\"contracheque-{mes}.pdf\"\r\n"
                    ),
                    "application/pdf",
                    &pdf_do_contracheque(mes),
                )
            } else {
                responder(&mut c, "404 Not Found", "", "text/plain", b"nao encontrado")
            }
        }
        // Os distratores: nada disto pode ser alcançado por um roteiro que respeita a lista de
        // bloqueio.
        (m, "/boleto/pagar" | "/cadastro/alterar") => {
            travar(&e.proibidos).push(format!("{m} {rota}"));
            html_ok(&mut c, "Feito", "<p>Operação realizada.</p>")
        }
        (_, "/codigo" | "/contracheques") if com_sessao => redirecionar(&mut c, "/codigo", None),
        (_, "/codigo" | "/contracheques") => redirecionar(&mut c, "/entrar", None),
        (_, r) if r.starts_with("/contracheque/") => redirecionar(&mut c, "/entrar", None),
        _ => responder(&mut c, "404 Not Found", "", "text/plain", b"nao encontrado"),
    }
}
