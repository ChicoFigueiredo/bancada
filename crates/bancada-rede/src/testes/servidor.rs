//! Servidor HTTP/1.1 falso, mínimo, em `127.0.0.1` porta efêmera: o suficiente para o `ureq`.
//! Uma thread por conexão, toda resposta fecha a conexão, e cada pedido é registrado antes da
//! resposta sair. As respostas vêm de rotas fixas ou de um tratador que o teste escreve.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::loopback::ler_consulta;
use std::collections::HashMap;
use std::fmt;
use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const CABECALHO_MAXIMO: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Jeito {
    Normal,
    /// Lê o pedido e fecha a conexão sem responder.
    Derrubar,
    /// Anuncia o corpo inteiro, manda só os primeiros `n` bytes e fecha.
    Cortar(usize),
    /// Espera `Duration` antes de responder.
    Demorar(Duration),
}

/// Resposta do servidor falso.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RespostaFalsa {
    status: u16,
    cabecalhos: Vec<(String, String)>,
    corpo: Vec<u8>,
    jeito: Jeito,
}

impl RespostaFalsa {
    pub fn status(status: u16) -> Self {
        Self {
            status,
            cabecalhos: Vec::new(),
            corpo: Vec::new(),
            jeito: Jeito::Normal,
        }
    }

    pub fn ok(corpo: impl Into<Vec<u8>>) -> Self {
        Self::status(200).com_corpo(corpo)
    }

    pub fn json(corpo: &str) -> Self {
        Self::ok(corpo.as_bytes().to_vec()).cabecalho("content-type", "application/json")
    }

    /// Erro com corpo JSON (por exemplo `401` com `{"codigo":"estacao_revogada"}`).
    pub fn erro_json(status: u16, corpo: &str) -> Self {
        Self::status(status)
            .com_corpo(corpo.as_bytes().to_vec())
            .cabecalho("content-type", "application/json")
    }

    /// Lê o pedido inteiro e derruba a conexão sem responder.
    pub fn derrubar() -> Self {
        Self {
            jeito: Jeito::Derrubar,
            ..Self::status(200)
        }
    }

    /// Anuncia o corpo inteiro, mas manda só os primeiros `bytes` e fecha (queda no meio).
    pub fn cortar_apos(mut self, bytes: usize) -> Self {
        self.jeito = Jeito::Cortar(bytes);
        self
    }

    /// Demora antes de responder (tempo real).
    pub fn demorar(mut self, quanto: Duration) -> Self {
        self.jeito = Jeito::Demorar(quanto);
        self
    }

    pub fn cabecalho(mut self, nome: &str, valor: &str) -> Self {
        self.cabecalhos
            .push((nome.to_ascii_lowercase(), valor.to_owned()));
        self
    }

    pub fn com_corpo(mut self, corpo: impl Into<Vec<u8>>) -> Self {
        self.corpo = corpo.into();
        self
    }
}

/// Um pedido que chegou ao servidor falso.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PedidoRecebido {
    pub metodo: String,
    pub caminho: String,
    /// Parâmetros de consulta já decodificados, na ordem recebida.
    pub consulta: Vec<(String, String)>,
    /// Cabeçalhos com o nome em minúsculas.
    pub cabecalhos: Vec<(String, String)>,
    pub corpo: Vec<u8>,
}

impl PedidoRecebido {
    pub fn cabecalho(&self, nome: &str) -> Option<&str> {
        let nome = nome.to_ascii_lowercase();
        self.cabecalhos
            .iter()
            .find(|(k, _)| *k == nome)
            .map(|(_, v)| v.as_str())
    }

    pub fn valor_da_consulta(&self, chave: &str) -> Option<&str> {
        self.consulta
            .iter()
            .find(|(k, _)| k == chave)
            .map(|(_, v)| v.as_str())
    }

    pub fn corpo_texto(&self) -> &str {
        std::str::from_utf8(&self.corpo).unwrap_or_default()
    }
}

type Tratador = dyn Fn(&PedidoRecebido) -> RespostaFalsa + Send + Sync;

#[derive(Debug, Default)]
struct Rota {
    respostas: Vec<RespostaFalsa>,
    proxima: usize,
}

struct Estado {
    rotas: Mutex<HashMap<String, Rota>>,
    tratador: Mutex<Option<Arc<Tratador>>>,
    pedidos: Mutex<Vec<PedidoRecebido>>,
    desligar: AtomicBool,
}

fn travar<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Servidor HTTP falso. Cai (e libera a porta) quando é descartado.
pub struct ServidorFalso {
    endereco: SocketAddr,
    estado: Arc<Estado>,
    aceitar: Option<JoinHandle<()>>,
}

impl fmt::Debug for ServidorFalso {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ServidorFalso")
            .field("endereco", &self.endereco)
            .finish_non_exhaustive()
    }
}

impl ServidorFalso {
    pub fn iniciar() -> Self {
        let escuta = TcpListener::bind("127.0.0.1:0").expect("porta efêmera de loopback");
        escuta.set_nonblocking(true).expect("accept sem bloqueio");
        let endereco = escuta.local_addr().expect("endereço local");
        let estado = Arc::new(Estado {
            rotas: Mutex::new(HashMap::new()),
            tratador: Mutex::new(None),
            pedidos: Mutex::new(Vec::new()),
            desligar: AtomicBool::new(false),
        });
        let compartilhado = estado.clone();
        let aceitar = thread::spawn(move || laco_de_aceite(&escuta, &compartilhado));
        Self {
            endereco,
            estado,
            aceitar: Some(aceitar),
        }
    }

    /// `http://127.0.0.1:<porta>`, para `ConfiguracaoDaRede::rota`.
    pub fn base(&self) -> String {
        format!("http://{}", self.endereco)
    }

    /// Responde sempre `resposta` em `caminho` (sem a consulta).
    pub fn rota(&self, caminho: &str, resposta: RespostaFalsa) {
        self.rota_em_sequencia(caminho, vec![resposta]);
    }

    /// Responde em sequência; depois da última, repete a última.
    pub fn rota_em_sequencia(&self, caminho: &str, respostas: Vec<RespostaFalsa>) {
        travar(&self.estado.rotas).insert(
            caminho.to_owned(),
            Rota {
                respostas,
                proxima: 0,
            },
        );
    }

    /// Tratador para todo caminho sem rota fixa (servidor com estado, escrito pelo teste).
    pub fn tratador(
        &self,
        tratador: impl Fn(&PedidoRecebido) -> RespostaFalsa + Send + Sync + 'static,
    ) {
        *travar(&self.estado.tratador) = Some(Arc::new(tratador));
    }

    pub fn pedidos(&self) -> Vec<PedidoRecebido> {
        travar(&self.estado.pedidos).clone()
    }

    /// Esquece os pedidos registrados.
    pub fn esquecer_pedidos(&self) {
        travar(&self.estado.pedidos).clear();
    }

    /// Espera (tempo real) até chegarem `n` pedidos.
    pub fn esperar_pedidos(&self, n: usize, prazo: Duration) -> bool {
        let inicio = Instant::now();
        loop {
            if travar(&self.estado.pedidos).len() >= n {
                return true;
            }
            if inicio.elapsed() > prazo {
                return false;
            }
            thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for ServidorFalso {
    fn drop(&mut self) {
        self.estado.desligar.store(true, Ordering::SeqCst);
        if let Some(h) = self.aceitar.take() {
            let _ = h.join();
        }
    }
}

fn laco_de_aceite(escuta: &TcpListener, estado: &Arc<Estado>) {
    while !estado.desligar.load(Ordering::SeqCst) {
        match escuta.accept() {
            Ok((conexao, _)) => {
                let estado = estado.clone();
                thread::spawn(move || atender(conexao, &estado));
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(2));
            }
            Err(_) => return,
        }
    }
}

fn atender(mut conexao: TcpStream, estado: &Estado) {
    let _ = conexao.set_nonblocking(false);
    let _ = conexao.set_read_timeout(Some(Duration::from_secs(5)));
    let Some(pedido) = ler_pedido(&mut conexao) else {
        return;
    };
    let fixa = {
        let mut rotas = travar(&estado.rotas);
        match rotas.get_mut(&pedido.caminho) {
            Some(rota) if !rota.respostas.is_empty() => {
                let i = rota.proxima.min(rota.respostas.len() - 1);
                rota.proxima += 1;
                Some(rota.respostas[i].clone())
            }
            _ => None,
        }
    };
    let resposta = fixa.unwrap_or_else(|| {
        let tratador = travar(&estado.tratador).clone();
        tratador.map_or_else(|| RespostaFalsa::status(404), |t| t(&pedido))
    });
    travar(&estado.pedidos).push(pedido);
    match resposta.jeito {
        Jeito::Derrubar => {}
        Jeito::Normal => escrever(&mut conexao, &resposta, resposta.corpo.len()),
        Jeito::Demorar(quanto) => {
            thread::sleep(quanto);
            escrever(&mut conexao, &resposta, resposta.corpo.len());
        }
        Jeito::Cortar(n) => escrever(&mut conexao, &resposta, n.min(resposta.corpo.len())),
    }
    let _ = conexao.shutdown(std::net::Shutdown::Both);
}

fn razao(status: u16) -> &'static str {
    match status {
        200 => "OK",
        304 => "Not Modified",
        401 => "Unauthorized",
        404 => "Not Found",
        409 => "Conflict",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Status",
    }
}

fn escrever(conexao: &mut TcpStream, resposta: &RespostaFalsa, enviar: usize) {
    let mut cabecalho = format!(
        "HTTP/1.1 {} {}\r\n",
        resposta.status,
        razao(resposta.status)
    );
    for (nome, valor) in &resposta.cabecalhos {
        cabecalho.push_str(&format!("{nome}: {valor}\r\n"));
    }
    if resposta.status != 304 {
        cabecalho.push_str(&format!("content-length: {}\r\n", resposta.corpo.len()));
    }
    cabecalho.push_str("connection: close\r\n\r\n");
    let _ = conexao
        .write_all(cabecalho.as_bytes())
        .and_then(|()| conexao.write_all(&resposta.corpo[..enviar]))
        .and_then(|()| conexao.flush());
}

fn ler_pedido(conexao: &mut TcpStream) -> Option<PedidoRecebido> {
    let mut dados = Vec::new();
    let mut buf = [0u8; 4096];
    let fim = loop {
        if let Some(i) = dados.windows(4).position(|w| w == b"\r\n\r\n") {
            break i;
        }
        if dados.len() > CABECALHO_MAXIMO {
            return None;
        }
        match conexao.read(&mut buf) {
            Ok(0) | Err(_) => return None,
            Ok(n) => dados.extend_from_slice(&buf[..n]),
        }
    };
    let cabecalho = String::from_utf8_lossy(&dados[..fim]).into_owned();
    let mut linhas = cabecalho.split("\r\n");
    let mut primeira = linhas.next()?.split(' ');
    let metodo = primeira.next()?.to_owned();
    let alvo = primeira.next()?.to_owned();
    let cabecalhos: Vec<(String, String)> = linhas
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_owned()))
        .collect();
    let tamanho: usize = cabecalhos
        .iter()
        .find(|(k, _)| k == "content-length")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    let mut corpo = dados[fim + 4..].to_vec();
    while corpo.len() < tamanho {
        match conexao.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => corpo.extend_from_slice(&buf[..n]),
        }
    }
    let (caminho, consulta) = match alvo.split_once('?') {
        Some((p, q)) => (p.to_owned(), ler_consulta(q)),
        None => (alvo, Vec::new()),
    };
    Some(PedidoRecebido {
        metodo,
        caminho,
        consulta,
        cabecalhos,
        corpo,
    })
}
