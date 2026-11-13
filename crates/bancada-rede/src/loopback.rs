//! Servidor de loopback do retorno da autorização pelo navegador (OAuth2 de app nativo,
//! RFC 8252 §7.3). É a **única escuta de rede** da bancada, e por isso é mínimo:
//!
//! - escuta só em `127.0.0.1`, porta efêmera escolhida pelo sistema;
//! - aceita conexões só de endereço de loopback e só com `Host: 127.0.0.1:<porta>` (contra
//!   *DNS rebinding*);
//! - processa **um único** retorno (`GET` no caminho combinado); depois fecha a porta.
//!   Pedidos a outro caminho (o `favicon` do navegador) recebem `404` e não contam;
//! - tem prazo (relógio injetado) e cancelamento;
//! - responde uma página mínima, sem script nem recurso externo: "Pode voltar ao <app>".
//!
//! Quem valida o `state` e troca o código é o fluxo ([`crate::FluxoOAuth`]); aqui só se recebe
//! o retorno.

use crate::relogio::{Instante, Relogio};
use std::fmt;
use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

/// Intervalo entre as consultas ao prazo e ao cancelamento (tempo real).
const INTERVALO: Duration = Duration::from_millis(10);
/// Prazo **total** (tempo real, desde o `accept`) para uma conexão mandar o cabeçalho inteiro.
const PRAZO_DO_CABECALHO: Duration = Duration::from_secs(3);
/// Fatia de cada leitura: entre uma e outra o cancelamento e o prazo são conferidos.
const FATIA_DE_LEITURA: Duration = Duration::from_millis(50);
const PRAZO_DE_ESCRITA: Duration = Duration::from_secs(2);
const CABECALHO_MAXIMO: usize = 8 * 1024;

/// Por que o retorno não chegou.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroDoRetorno {
    /// Não foi possível escutar em `127.0.0.1`, ou o caminho não é um caminho simples.
    Indisponivel,
    /// O prazo passou sem retorno.
    Expirado,
    /// Quem esperava desistiu.
    Cancelado,
}

impl fmt::Display for ErroDoRetorno {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Indisponivel => {
                "não foi possível receber o retorno do navegador (porta local indisponível)"
            }
            Self::Expirado => "o prazo para concluir a autorização no navegador passou",
            Self::Cancelado => "autorização cancelada",
        })
    }
}

impl std::error::Error for ErroDoRetorno {}

/// Parâmetros do retorno (`?code=…&state=…` ou `?error=…&state=…`), já decodificados. O
/// `Debug` mostra só os nomes: o código de autorização não vai para log.
#[derive(Clone, PartialEq, Eq)]
pub struct ParametrosDoRetorno {
    pares: Vec<(String, String)>,
}

impl ParametrosDoRetorno {
    /// Primeiro valor de `chave`.
    pub fn valor(&self, chave: &str) -> Option<&str> {
        self.pares
            .iter()
            .find(|(k, _)| k == chave)
            .map(|(_, v)| v.as_str())
    }

    /// Quantas vezes `chave` aparece (um retorno legítimo traz cada uma no máximo uma vez).
    pub fn quantas(&self, chave: &str) -> usize {
        self.pares.iter().filter(|(k, _)| k == chave).count()
    }
}

impl fmt::Debug for ParametrosDoRetorno {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let chaves: Vec<&str> = self.pares.iter().map(|(k, _)| k.as_str()).collect();
        f.debug_struct("ParametrosDoRetorno")
            .field("chaves", &chaves)
            .finish()
    }
}

/// O servidor de loopback, aberto e esperando um retorno. Descartar fecha a porta.
pub struct RetornoPorLoopback {
    escuta: TcpListener,
    porta: u16,
    caminho: String,
    pagina: String,
}

impl fmt::Debug for RetornoPorLoopback {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RetornoPorLoopback")
            .field("porta", &self.porta)
            .field("caminho", &self.caminho)
            .finish_non_exhaustive()
    }
}

/// Só conexões vindas do próprio computador.
fn aceita_par(par: &SocketAddr) -> bool {
    par.ip().is_loopback()
}

fn escapar_html(texto: &str) -> String {
    texto
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

impl RetornoPorLoopback {
    /// Escuta em `127.0.0.1` numa porta efêmera; o retorno é esperado em `caminho` (por exemplo
    /// `/sbf/retorno`). A página de resposta diz "Pode voltar ao `nome_do_app`".
    ///
    /// # Errors
    /// [`ErroDoRetorno::Indisponivel`].
    pub fn escutar(caminho: &str, nome_do_app: &str) -> Result<Self, ErroDoRetorno> {
        if !caminho.starts_with('/') || caminho.contains(['?', '#', ' ', '\r', '\n']) {
            return Err(ErroDoRetorno::Indisponivel);
        }
        let endereco = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        let escuta = TcpListener::bind(endereco).map_err(|_| ErroDoRetorno::Indisponivel)?;
        escuta
            .set_nonblocking(true)
            .map_err(|_| ErroDoRetorno::Indisponivel)?;
        let porta = escuta
            .local_addr()
            .map_err(|_| ErroDoRetorno::Indisponivel)?
            .port();
        let nome = escapar_html(nome_do_app);
        let pagina = format!(
            "<!doctype html><html lang=\"pt-BR\"><head><meta charset=\"utf-8\">\
             <title>{nome}</title></head><body><p>Pode voltar ao {nome}.</p></body></html>"
        );
        Ok(Self {
            escuta,
            porta,
            caminho: caminho.to_owned(),
            pagina,
        })
    }

    pub fn porta(&self) -> u16 {
        self.porta
    }

    /// `http://127.0.0.1:<porta><caminho>`: o `redirect_uri` da autorização. Sempre o IP
    /// literal, nunca `localhost` (que pode resolver para outro endereço).
    pub fn endereco_de_retorno(&self) -> String {
        format!("http://127.0.0.1:{}{}", self.porta, self.caminho)
    }

    /// Espera o único retorno até `prazo` (no relógio `relogio`) ou até `cancelar`. Consome o
    /// servidor: ao voltar, a porta já está fechada.
    ///
    /// # Errors
    /// [`ErroDoRetorno::Expirado`] ou [`ErroDoRetorno::Cancelado`].
    pub fn esperar(
        self,
        relogio: &dyn Relogio,
        prazo: Instante,
        cancelar: &AtomicBool,
    ) -> Result<ParametrosDoRetorno, ErroDoRetorno> {
        loop {
            if cancelar.load(Ordering::Acquire) {
                return Err(ErroDoRetorno::Cancelado);
            }
            if relogio.agora() >= prazo {
                return Err(ErroDoRetorno::Expirado);
            }
            match self.escuta.accept() {
                Ok((conexao, par)) => {
                    if !aceita_par(&par) {
                        continue;
                    }
                    if let Some(parametros) = self.atender(conexao, relogio, prazo, cancelar) {
                        return Ok(parametros);
                    }
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => thread::sleep(INTERVALO),
                Err(_) => thread::sleep(INTERVALO),
            }
        }
    }

    /// Atende uma conexão. `Some` só para o retorno (o `GET` no caminho combinado, com o
    /// `Host` certo); o resto recebe `404` e não conta.
    fn atender(
        &self,
        mut conexao: TcpStream,
        relogio: &dyn Relogio,
        prazo: Instante,
        cancelar: &AtomicBool,
    ) -> Option<ParametrosDoRetorno> {
        let _ = conexao.set_nonblocking(false);
        let _ = conexao.set_read_timeout(Some(FATIA_DE_LEITURA));
        let _ = conexao.set_write_timeout(Some(PRAZO_DE_ESCRITA));
        let cabecalho = ler_cabecalho(&mut conexao, relogio, prazo, cancelar)?;
        let mut linhas = cabecalho.split("\r\n");
        let mut primeira = linhas.next()?.split(' ');
        let (metodo, alvo) = (primeira.next()?, primeira.next()?);
        let host_esperado = format!("127.0.0.1:{}", self.porta);
        let host_ok = linhas
            .filter_map(|l| l.split_once(':'))
            .filter(|(k, _)| k.trim().eq_ignore_ascii_case("host"))
            .map(|(_, v)| v.trim())
            .collect::<Vec<_>>()
            == [host_esperado.as_str()];
        let (caminho, consulta) = alvo.split_once('?').unwrap_or((alvo, ""));
        if metodo != "GET" || caminho != self.caminho || !host_ok {
            responder(&mut conexao, "404 Not Found", "");
            return None;
        }
        responder(&mut conexao, "200 OK", &self.pagina);
        Some(ParametrosDoRetorno {
            pares: ler_consulta(consulta),
        })
    }
}

fn ler_cabecalho(
    conexao: &mut TcpStream,
    relogio: &dyn Relogio,
    prazo: Instante,
    cancelar: &AtomicBool,
) -> Option<String> {
    let inicio = Instant::now();
    let mut dados = Vec::new();
    let mut buf = [0u8; 1024];
    loop {
        if inicio.elapsed() >= PRAZO_DO_CABECALHO
            || cancelar.load(Ordering::Acquire)
            || relogio.agora() >= prazo
        {
            return None;
        }
        if let Some(fim) = dados.windows(4).position(|w| w == b"\r\n\r\n") {
            return String::from_utf8(dados[..fim].to_vec()).ok();
        }
        if dados.len() > CABECALHO_MAXIMO {
            return None;
        }
        match conexao.read(&mut buf) {
            Ok(0) => return None,
            Ok(n) => dados.extend_from_slice(&buf[..n]),
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(_) => return None,
        }
    }
}

fn responder(conexao: &mut TcpStream, status: &str, corpo: &str) {
    let cabecalho = format!(
        "HTTP/1.1 {status}\r\n\
         content-type: text/html; charset=utf-8\r\n\
         content-length: {}\r\n\
         cache-control: no-store\r\n\
         referrer-policy: no-referrer\r\n\
         x-content-type-options: nosniff\r\n\
         content-security-policy: default-src 'none'\r\n\
         connection: close\r\n\r\n",
        corpo.len()
    );
    let _ = conexao
        .write_all(cabecalho.as_bytes())
        .and_then(|()| conexao.write_all(corpo.as_bytes()))
        .and_then(|()| conexao.flush());
    let _ = conexao.shutdown(std::net::Shutdown::Both);
}

pub(crate) fn ler_consulta(consulta: &str) -> Vec<(String, String)> {
    consulta
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|par| {
            let (k, v) = par.split_once('=').unwrap_or((par, ""));
            (decodificar(k), decodificar(v))
        })
        .collect()
}

/// Decodificação de consulta: `%XX` e `+` (espaço).
pub(crate) fn decodificar(texto: &str) -> String {
    let bytes = texto.as_bytes();
    let mut saida = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                if let Some(byte) = texto
                    .get(i + 1..i + 3)
                    .and_then(|h| u8::from_str_radix(h, 16).ok())
                {
                    saida.push(byte);
                    i += 3;
                    continue;
                }
                saida.push(b'%');
            }
            b'+' => saida.push(b' '),
            b => saida.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&saida).into_owned()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn so_pares_de_loopback_sao_aceitos() {
        for ok in ["127.0.0.1:5000", "127.8.9.10:1", "[::1]:80"] {
            let par: SocketAddr = ok.parse().expect("endereço");
            assert!(aceita_par(&par), "{ok}");
        }
        for ruim in [
            "192.168.0.10:5000",
            "10.0.0.1:1",
            "0.0.0.0:2",
            "[2001:db8::1]:80",
        ] {
            let par: SocketAddr = ruim.parse().expect("endereço");
            assert!(!aceita_par(&par), "{ruim}");
        }
    }

    #[test]
    fn valores_da_consulta_sao_decodificados() {
        assert_eq!(
            ler_consulta("code=a%2Fb&state=x+y&error=&solo"),
            vec![
                ("code".to_owned(), "a/b".to_owned()),
                ("state".to_owned(), "x y".to_owned()),
                ("error".to_owned(), String::new()),
                ("solo".to_owned(), String::new()),
            ]
        );
    }

    #[test]
    fn caminho_ruim_e_recusado() {
        assert!(RetornoPorLoopback::escutar("sem-barra", "App").is_err());
        assert!(RetornoPorLoopback::escutar("/a?b", "App").is_err());
    }

    #[test]
    fn o_nome_do_app_e_escapado_na_pagina() {
        let r = RetornoPorLoopback::escutar("/r", "<b>App</b>").expect("escuta");
        assert!(r.pagina.contains("&lt;b&gt;App&lt;/b&gt;"));
        assert!(r.endereco_de_retorno().starts_with("http://127.0.0.1:"));
    }
}
