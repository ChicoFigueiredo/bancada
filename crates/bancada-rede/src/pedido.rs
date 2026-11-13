//! O pedido: endereço HTTPS oficial + opções. Quem chama escreve a URL real; o portão a
//! confere no catálogo e, nos testes, troca o host pelo servidor falso.

use crate::erro::ErroDeRede;
use std::fmt;
use std::time::Duration;
use zeroize::Zeroizing;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Metodo {
    Get,
    Post,
    Delete,
}

impl Metodo {
    pub const fn como_texto(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Delete => "DELETE",
        }
    }
}

/// Cabeçalhos que nunca vêm de quem chama: a autorização é da fonte de autorização, a
/// revalidação é do portão, cookie não se usa.
const CABECALHOS_PROIBIDOS: [&str; 6] = [
    "authorization",
    "proxy-authorization",
    "cookie",
    "host",
    "if-none-match",
    "if-modified-since",
];

/// Um pedido. Construa com [`Pedido::get`], [`Pedido::post`] ou [`Pedido::delete`] e envie
/// com `Rede::buscar` (sem bloquear) ou `Rede::executar` (bloqueando a thread de quem chama).
/// O `Debug` mostra o tamanho do corpo, não o conteúdo (a troca de código por token leva o
/// código e o verificador).
#[derive(Clone)]
pub struct Pedido {
    pub(crate) metodo: Metodo,
    pub(crate) url: String,
    pub(crate) consulta: Vec<(String, String)>,
    pub(crate) cabecalhos: Vec<(String, String)>,
    pub(crate) corpo: Option<(String, Zeroizing<Vec<u8>>)>,
    pub(crate) autorizado: bool,
    pub(crate) guardar_por: Option<Duration>,
    pub(crate) etag_conhecida: Option<String>,
    pub(crate) aceitar_vencido: bool,
    pub(crate) renovar: bool,
    pub(crate) paciencia: Duration,
    pub(crate) prazo: Option<Duration>,
    pub(crate) limite_de_corpo: Option<usize>,
    pub(crate) invalido: Option<String>,
}

impl Pedido {
    pub fn get(url: impl Into<String>) -> Self {
        Self::novo(Metodo::Get, url.into())
    }

    pub fn post(url: impl Into<String>) -> Self {
        Self::novo(Metodo::Post, url.into())
    }

    pub fn delete(url: impl Into<String>) -> Self {
        Self::novo(Metodo::Delete, url.into())
    }

    fn novo(metodo: Metodo, url: String) -> Self {
        Self {
            metodo,
            url,
            consulta: Vec::new(),
            cabecalhos: Vec::new(),
            corpo: None,
            autorizado: false,
            guardar_por: None,
            etag_conhecida: None,
            aceitar_vencido: true,
            renovar: false,
            paciencia: Duration::ZERO,
            prazo: None,
            limite_de_corpo: None,
            invalido: None,
        }
    }

    pub fn metodo(&self) -> Metodo {
        self.metodo
    }

    /// Parâmetro de consulta (codificado no envio; a ordem não muda a chave do cache).
    pub fn consulta(mut self, chave: impl Into<String>, valor: impl Into<String>) -> Self {
        self.consulta.push((chave.into(), valor.into()));
        self
    }

    /// Cabeçalho extra. `Authorization`, `Cookie`, `Host` e os de revalidação são recusados:
    /// a autorização vem da fonte de autorização e a revalidação, do portão.
    pub fn cabecalho(mut self, nome: impl Into<String>, valor: impl Into<String>) -> Self {
        let nome = nome.into();
        let minusculo = nome.to_ascii_lowercase();
        let valor = valor.into();
        if CABECALHOS_PROIBIDOS.contains(&minusculo.as_str()) {
            self.invalido = Some(format!(
                "o cabeçalho {nome} não pode ser definido por quem pede"
            ));
            return self;
        }
        if valor.chars().any(char::is_control) || minusculo.is_empty() {
            self.invalido = Some(format!("o cabeçalho {nome} tem caractere de controle"));
            return self;
        }
        self.cabecalhos.push((minusculo, valor));
        self
    }

    pub fn aceitar(self, tipo: impl Into<String>) -> Self {
        self.cabecalho("accept", tipo)
    }

    /// Corpo, com o tipo de conteúdo. A memória do corpo é zerada ao descartar o pedido.
    pub fn corpo(mut self, tipo: impl Into<String>, bytes: Vec<u8>) -> Self {
        self.corpo = Some((tipo.into(), Zeroizing::new(bytes)));
        self
    }

    /// Corpo `application/json`.
    pub fn json(self, bytes: Vec<u8>) -> Self {
        self.corpo("application/json", bytes)
    }

    /// Corpo `application/x-www-form-urlencoded` com os pares dados, montado sem cópia
    /// solta (os valores podem ser segredo: código e verificador do OAuth).
    pub fn formulario(self, pares: &[(&str, &str)]) -> Self {
        let tamanho: usize = pares
            .iter()
            .map(|(k, v)| codificado_len(k) + codificado_len(v) + 2)
            .sum();
        let mut corpo = Vec::with_capacity(tamanho);
        for (i, (k, v)) in pares.iter().enumerate() {
            if i > 0 {
                corpo.push(b'&');
            }
            codificar_em(k, &mut corpo);
            corpo.push(b'=');
            codificar_em(v, &mut corpo);
        }
        self.corpo("application/x-www-form-urlencoded", corpo)
    }

    /// O pedido leva o cabeçalho `Authorization` que a fonte de autorização der para o
    /// endereço. Sem credencial, o pedido falha antes de sair ([`ErroDeRede::SemCredencial`]),
    /// nunca sai sem o cabeçalho em silêncio.
    pub fn autorizado(mut self) -> Self {
        self.autorizado = true;
        self
    }

    /// Guarda a resposta `200` no cache do portão por `validade`.
    pub fn guardar_por(mut self, validade: Duration) -> Self {
        self.guardar_por = Some(validade);
        self
    }

    /// Pedido condicional com uma `ETag` que **quem chama** guarda (`If-None-Match`). Se o
    /// servidor responder `304`, a resposta volta com [`crate::Frescor::NaoMudou`] e corpo
    /// vazio. Para quem tem o próprio cache (substituído numa transação só).
    pub fn se_mudou_desde(mut self, etag: impl Into<String>) -> Self {
        let etag = etag.into();
        if etag.chars().any(char::is_control) {
            self.invalido = Some("ETag com caractere de controle".to_owned());
        } else {
            self.etag_conhecida = Some(etag);
        }
        self
    }

    /// Consulta o servidor mesmo que o cache do portão ainda esteja válido ("Atualizar").
    pub fn renovar(mut self) -> Self {
        self.renovar = true;
        self
    }

    /// Sem rede, não devolver o cache vencido (o padrão devolve, marcado `Vencida`).
    pub fn sem_vencido(mut self) -> Self {
        self.aceitar_vencido = false;
        self
    }

    /// Quanto o pedido aceita esperar por uma pausa pedida pelo servidor (`429`). O padrão é
    /// zero: falha na hora com [`ErroDeRede::Limitado`].
    pub fn paciencia(mut self, espera: Duration) -> Self {
        self.paciencia = espera;
        self
    }

    /// Prazo total do pedido (padrão da configuração).
    pub fn prazo(mut self, prazo: Duration) -> Self {
        self.prazo = Some(prazo);
        self
    }

    /// Limite de bytes da resposta (padrão da configuração).
    pub fn limite_de_corpo(mut self, bytes: usize) -> Self {
        self.limite_de_corpo = Some(bytes);
        self
    }

    /// Endereço validado: HTTPS, host e caminho bem formados, sem userinfo nem consulta crua.
    pub(crate) fn alvo(&self) -> Result<Alvo, ErroDeRede> {
        if let Some(motivo) = &self.invalido {
            return Err(ErroDeRede::invalido(motivo.clone()));
        }
        if self.metodo == Metodo::Get && self.corpo.is_some() {
            return Err(ErroDeRede::invalido("GET não leva corpo"));
        }
        ler_https(&self.url).ok_or_else(|| ErroDeRede::EnderecoForaDoCatalogo {
            endereco: sanitizado(&self.url),
        })
    }

    /// Consulta codificada, em ordem estável pela chave.
    pub(crate) fn consulta_codificada(&self) -> String {
        let mut pares: Vec<&(String, String)> = self.consulta.iter().collect();
        pares.sort_by(|a, b| a.0.cmp(&b.0));
        pares
            .iter()
            .map(|(k, v)| format!("{}={}", codificar(k), codificar(v)))
            .collect::<Vec<_>>()
            .join("&")
    }

    /// URL oficial (sem segredo): a da origem e a da chave do cache.
    pub(crate) fn url_canonica(&self, alvo: &Alvo) -> String {
        let consulta = self.consulta_codificada();
        let base = format!("https://{}{}", alvo.autoridade(), alvo.caminho);
        if consulta.is_empty() {
            base
        } else {
            format!("{base}?{consulta}")
        }
    }

    /// Chave do cache: método, URL normalizada, cabeçalhos, autorização e resumo do corpo.
    pub(crate) fn chave_do_cache(&self, alvo: &Alvo) -> String {
        let mut cabecalhos = self.cabecalhos.clone();
        cabecalhos.sort();
        let cabecalhos: Vec<String> = cabecalhos.iter().map(|(k, v)| format!("{k}:{v}")).collect();
        let corpo = self
            .corpo
            .as_ref()
            .map(|(t, b)| format!("{t}:{:016x}", fnv1a(b)))
            .unwrap_or_default();
        format!(
            "{} {}\ncabecalhos={}\nautorizado={}\ncorpo={corpo}",
            self.metodo.como_texto(),
            self.url_canonica(alvo),
            cabecalhos.join(","),
            self.autorizado,
        )
    }
}

impl fmt::Debug for Pedido {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Pedido")
            .field("metodo", &self.metodo)
            .field("url", &sanitizado(&self.url))
            .field("consulta", &self.consulta.len())
            .field("cabecalhos", &self.cabecalhos)
            .field(
                "corpo",
                &self
                    .corpo
                    .as_ref()
                    .map(|(t, b)| format!("{t}, {} bytes", b.len())),
            )
            .field("autorizado", &self.autorizado)
            .field("guardar_por", &self.guardar_por)
            .field("etag_conhecida", &self.etag_conhecida.is_some())
            .field("prazo", &self.prazo)
            .field("invalido", &self.invalido)
            .finish_non_exhaustive()
    }
}

/// Host, porta e caminho de uma URL HTTPS já validada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Alvo {
    pub host: String,
    pub porta: Option<u16>,
    pub caminho: String,
}

impl Alvo {
    pub fn autoridade(&self) -> String {
        match self.porta {
            Some(porta) => format!("{}:{porta}", self.host),
            None => self.host.clone(),
        }
    }
}

/// Aceita só `https://host[:porta]/caminho`, com host em minúsculas de `[a-z0-9.-]`, sem
/// userinfo, consulta (vai por `consulta`) ou fragmento.
pub(crate) fn ler_https(url: &str) -> Option<Alvo> {
    let resto = url.strip_prefix("https://")?;
    let (autoridade, caminho) = match resto.find('/') {
        Some(i) => (&resto[..i], &resto[i..]),
        None => (resto, "/"),
    };
    let (host, porta) = match autoridade.rsplit_once(':') {
        Some((h, p)) => (h, Some(p.parse::<u16>().ok()?)),
        None => (autoridade, None),
    };
    let host = host.to_ascii_lowercase();
    let host_ok = !host.is_empty()
        && host
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-')
        && !host.starts_with('.')
        && !host.ends_with('.');
    let caminho_ok = caminho
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"-._~!$&'()*+,;=:@/%".contains(&b));
    let porta = porta.filter(|p| *p != 443);
    (host_ok && caminho_ok && caminho_literal(caminho)).then(|| Alvo {
        host,
        porta,
        caminho: caminho.to_owned(),
    })
}

/// Caminho sem segmentos `.`/`..` e sem escapes que escondem `.`, `/` ou `\`: o catálogo
/// classifica o pedido pelo prefixo do caminho, e um servidor que normalize `..` responderia
/// outro endereço.
fn caminho_literal(caminho: &str) -> bool {
    let minusculo = caminho.to_ascii_lowercase();
    !["%2e", "%2f", "%5c"].iter().any(|e| minusculo.contains(e))
        && !caminho
            .split('/')
            .any(|segmento| segmento == "." || segmento == "..")
}

/// URL para mensagens: sem consulta e cortada.
pub(crate) fn sanitizado(url: &str) -> String {
    let base = url.split(['?', '#']).next().unwrap_or_default();
    base.chars().take(120).collect()
}

fn nao_reservado(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~')
}

/// Percent-encoding de componente (RFC 3986): só os não reservados passam.
pub fn codificar(texto: &str) -> String {
    let mut saida = Vec::with_capacity(codificado_len(texto));
    codificar_em(texto, &mut saida);
    String::from_utf8(saida).unwrap_or_default()
}

fn codificado_len(texto: &str) -> usize {
    texto
        .bytes()
        .map(|b| if nao_reservado(b) { 1 } else { 3 })
        .sum()
}

fn codificar_em(texto: &str, saida: &mut Vec<u8>) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for b in texto.bytes() {
        if nao_reservado(b) {
            saida.push(b);
        } else {
            saida.push(b'%');
            saida.push(HEX[usize::from(b >> 4)]);
            saida.push(HEX[usize::from(b & 0xF)]);
        }
    }
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn so_https_bem_formado_passa() {
        assert!(ler_https("https://sbf.exemplo.com.br/api/v1/estado").is_some());
        let alvo = ler_https("https://sbf.exemplo.com.br:8443/x").expect("alvo");
        assert_eq!(alvo.porta, Some(8443));
        assert_eq!(
            ler_https("https://sbf.exemplo.com.br:443/x").and_then(|a| a.porta),
            None
        );
        for ruim in [
            "http://sbf.exemplo.com.br/api",
            "https://usuario@sbf.exemplo.com.br/api",
            "https://sbf.exemplo.com.br/api?x=1",
            "https://sbf.exemplo.com.br/a/../b",
            "https://sbf.exemplo.com.br/a/%2e%2e/b",
            "https://sbf.exemplo.com.br/a b",
            "https://.exemplo/a",
            "ftp://exemplo/a",
            "",
        ] {
            assert!(ler_https(ruim).is_none(), "{ruim}");
        }
    }

    #[test]
    fn cabecalho_de_autorizacao_nao_vem_de_quem_pede() {
        let pedido = Pedido::get("https://a.b/c").cabecalho("Authorization", "Bearer x");
        assert!(matches!(
            pedido.alvo(),
            Err(ErroDeRede::PedidoInvalido { .. })
        ));
        let pedido = Pedido::get("https://a.b/c").cabecalho("If-None-Match", "\"1\"");
        assert!(pedido.alvo().is_err());
    }

    #[test]
    fn o_debug_nao_mostra_o_corpo() {
        let pedido = Pedido::post("https://a.b/token").formulario(&[("code", "segredo-123")]);
        let texto = format!("{pedido:?}");
        assert!(!texto.contains("segredo-123"), "{texto}");
    }

    #[test]
    fn o_formulario_e_codificado() {
        let pedido = Pedido::post("https://a.b/t").formulario(&[("a", "x y"), ("b", "1/2")]);
        let (_, corpo) = pedido.corpo.expect("corpo");
        assert_eq!(corpo.as_slice(), b"a=x%20y&b=1%2F2");
    }
}
