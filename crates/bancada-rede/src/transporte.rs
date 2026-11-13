//! Transporte HTTP (`ureq` + rustls). É o único arquivo do crate que fala `ureq`, e só o
//! portão o chama: nenhum caminho chega aqui sem passar pelo catálogo e pelos limites. Não
//! segue redirecionamentos.

use crate::pedido::Metodo;
use std::io::{ErrorKind, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use zeroize::Zeroizing;

/// O pedido pronto para sair (endereço já roteado, cabeçalhos já filtrados pelo portão).
pub(crate) struct Saida<'a> {
    pub metodo: Metodo,
    pub url: String,
    pub cabecalhos: Vec<(String, String)>,
    /// Valor do `Authorization`, já conferido contra o endereço pelo portão.
    pub autorizacao: Option<&'a str>,
    pub corpo: Option<&'a (String, Zeroizing<Vec<u8>>)>,
    pub prazo: Duration,
    pub limite_de_corpo: usize,
    pub cancelar: &'a AtomicBool,
}

/// Cabeçalhos da resposta que o portão usa.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Cabeca {
    pub status: u16,
    pub tipo: Option<String>,
    pub etag: Option<String>,
    pub tentar_de_novo: Option<String>,
}

#[derive(Debug)]
pub(crate) enum Falha {
    Prazo,
    /// Sem rota até o servidor (rede fora, DNS, recusada): não adianta repetir já.
    Inalcancavel(&'static str),
    /// Conexão caiu no meio: vale repetir.
    Interrompida(&'static str),
    CorpoGrandeDemais,
    Cancelado,
}

#[derive(Debug)]
pub(crate) struct Http {
    agente: ureq::Agent,
}

impl Http {
    pub fn novo(prazo_de_conexao: Duration, agente_de_usuario: &str) -> Self {
        let agente: ureq::Agent = ureq::Agent::config_builder()
            // Todo status chega como resposta: 304, 429 e 5xx são tratados pelo portão.
            .http_status_as_error(false)
            // Redirecionamento poderia levar a outro host (e a credencial junto): nunca seguir.
            .max_redirects(0)
            .user_agent(agente_de_usuario)
            .timeout_connect(Some(prazo_de_conexao))
            .build()
            .into();
        Self { agente }
    }

    /// Pedido completo: cabeçalhos e corpo (com limite e atenção ao cancelamento).
    pub fn enviar(&self, saida: &Saida<'_>) -> Result<(Cabeca, Vec<u8>), Falha> {
        if saida.cancelar.load(Ordering::Acquire) {
            return Err(Falha::Cancelado);
        }
        let resultado = match saida.metodo {
            Metodo::Get | Metodo::Delete => {
                let mut req = if saida.metodo == Metodo::Get {
                    self.agente.get(&saida.url)
                } else {
                    self.agente.delete(&saida.url)
                };
                for (nome, valor) in &saida.cabecalhos {
                    req = req.header(nome.as_str(), valor.as_str());
                }
                if let Some(autorizacao) = saida.autorizacao {
                    req = req.header("authorization", autorizacao);
                }
                req.config()
                    .timeout_global(Some(saida.prazo))
                    .build()
                    .call()
            }
            Metodo::Post => {
                let mut req = self.agente.post(&saida.url);
                for (nome, valor) in &saida.cabecalhos {
                    req = req.header(nome.as_str(), valor.as_str());
                }
                if let Some(autorizacao) = saida.autorizacao {
                    req = req.header("authorization", autorizacao);
                }
                let (tipo, bytes) = match saida.corpo {
                    Some((t, b)) => (t.as_str(), b.as_slice()),
                    None => ("application/octet-stream", &[][..]),
                };
                req.header("content-type", tipo)
                    .config()
                    .timeout_global(Some(saida.prazo))
                    .build()
                    .send(bytes)
            }
        };
        let resposta = resultado.map_err(falha)?;
        let cabeca = cabeca(&resposta);
        let mut leitor = resposta
            .into_body()
            .into_with_config()
            .limit(u64::MAX)
            .reader();
        let mut corpo = Vec::new();
        let mut pedaco = [0u8; 16 * 1024];
        loop {
            if saida.cancelar.load(Ordering::Acquire) {
                return Err(Falha::Cancelado);
            }
            match leitor.read(&mut pedaco) {
                Ok(0) => return Ok((cabeca, corpo)),
                Ok(n) => {
                    if corpo.len() + n > saida.limite_de_corpo {
                        return Err(Falha::CorpoGrandeDemais);
                    }
                    corpo.extend_from_slice(&pedaco[..n]);
                }
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(e) => return Err(falha_de_es(e.kind())),
            }
        }
    }
}

fn cabeca(resposta: &ureq::http::Response<ureq::Body>) -> Cabeca {
    let valor = |nome: &str| {
        resposta
            .headers()
            .get(nome)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
    };
    Cabeca {
        status: resposta.status().as_u16(),
        tipo: valor("content-type"),
        etag: valor("etag"),
        tentar_de_novo: valor("retry-after"),
    }
}

/// Mensagens fixas: o erro do `ureq` pode citar a URL, que não vai para o diagnóstico.
fn falha(erro: ureq::Error) -> Falha {
    match erro {
        ureq::Error::Timeout(_) => Falha::Prazo,
        ureq::Error::HostNotFound => Falha::Inalcancavel("host não encontrado"),
        ureq::Error::ConnectionFailed => Falha::Inalcancavel("conexão recusada"),
        ureq::Error::BodyExceedsLimit(_) => Falha::CorpoGrandeDemais,
        ureq::Error::Io(e) => falha_de_es(e.kind()),
        ureq::Error::Tls(_) | ureq::Error::Rustls(_) => Falha::Inalcancavel("falha de TLS"),
        ureq::Error::Protocol(_) => Falha::Interrompida("resposta HTTP malformada"),
        _ => Falha::Inalcancavel("falha de rede"),
    }
}

fn falha_de_es(tipo: ErrorKind) -> Falha {
    match tipo {
        ErrorKind::TimedOut | ErrorKind::WouldBlock => Falha::Prazo,
        ErrorKind::ConnectionRefused => Falha::Inalcancavel("conexão recusada"),
        ErrorKind::NotConnected
        | ErrorKind::AddrNotAvailable
        | ErrorKind::NetworkUnreachable
        | ErrorKind::HostUnreachable
        | ErrorKind::NetworkDown => Falha::Inalcancavel("rede indisponível"),
        _ => Falha::Interrompida("conexão interrompida"),
    }
}
