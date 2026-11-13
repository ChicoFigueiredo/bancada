//! OAuth2 de app nativo (RFC 8252): código de autorização com PKCE S256, retorno pelo
//! loopback, troca do código no endpoint de token pelo portão. Genérico: o provedor (Google,
//! por exemplo) é dado por quem chama, e o que se faz com o token (no SBF, o `id_token` vira
//! prova para parear a estação) também.
//!
//! O app nunca vê a senha do usuário. Abrir o navegador é da interface
//! ([`FluxoOAuth::abrir_no_navegador`]); esperar o retorno bloqueia, então roda fora da thread
//! da interface. Código, verificador e tokens vivem em [`Segredo`]: sem `Display`, `Debug`
//! oculto, memória zerada ao descartar.

use crate::catalogo::Endereco;
use crate::erro::ErroDeRede;
use crate::loopback::{ErroDoRetorno, RetornoPorLoopback};
use crate::navegador::abrir_no_navegador;
use crate::pedido::{Pedido, codificar, ler_https};
use crate::portao::Rede;
use crate::relogio::{Instante, Relogio};
use bancada_segredos::Segredo;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fmt;
use std::sync::atomic::AtomicBool;
use std::time::Duration;
use zeroize::Zeroizing;

/// Endpoint de autorização do Google (documentação oficial de OAuth 2.0 para apps nativos).
pub const GOOGLE_AUTORIZACAO: &str = "https://accounts.google.com/o/oauth2/v2/auth";
/// Endpoint de token do Google.
pub const GOOGLE_TOKEN: &str = "https://oauth2.googleapis.com/token";

/// Tamanho máximo aceito para o código de autorização.
const CODIGO_MAXIMO: usize = 4096;
const PRAZO_DA_TROCA: Duration = Duration::from_secs(30);

/// Por que a autorização não deu certo. Nenhuma variante carrega código, verificador ou token.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErroOAuth {
    /// Falha local: sem gerador aleatório, ou endereço do provedor que não é HTTPS.
    Local { detalhe: &'static str },
    /// O retorno pelo navegador não chegou.
    Retorno(ErroDoRetorno),
    /// O `state` do retorno não confere: retorno forjado ou de outra autorização.
    EstadoNaoConfere,
    /// O retorno não trouxe um código válido.
    RetornoMalformado,
    /// O usuário (ou o provedor) negou a autorização; `erro` é o código OAuth, sanitizado.
    Negada { erro: String },
    /// O provedor recusou o código (já usado, vencido ou de outra autorização).
    CodigoRecusado,
    /// A resposta do endpoint de token não tem o formato do OAuth.
    RespostaMalformada,
    /// Falha do portão (sem rede, fora do catálogo…).
    Rede(ErroDeRede),
}

impl fmt::Display for ErroOAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Local { detalhe } => write!(f, "falha local na autorização: {detalhe}"),
            Self::Retorno(e) => e.fmt(f),
            Self::EstadoNaoConfere => f.write_str(
                "o retorno do navegador não corresponde a esta autorização; nada foi aceito",
            ),
            Self::RetornoMalformado => f.write_str("o retorno do navegador veio incompleto"),
            Self::Negada { erro } => write!(f, "a autorização foi negada ({erro})"),
            Self::CodigoRecusado => {
                f.write_str("o provedor recusou o código de autorização (já usado ou vencido)")
            }
            Self::RespostaMalformada => {
                f.write_str("a resposta do provedor não tem o formato esperado")
            }
            Self::Rede(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for ErroOAuth {}

impl From<ErroDeRede> for ErroOAuth {
    fn from(erro: ErroDeRede) -> Self {
        Self::Rede(erro)
    }
}

/// O provedor de identidade: endpoints, cliente e escopos.
#[derive(Clone)]
pub struct ProvedorOAuth {
    endereco_de_autorizacao: String,
    endereco_de_token: String,
    id_do_cliente: String,
    segredo_do_cliente: Option<Segredo>,
    escopos: Vec<String>,
    extras: Vec<(String, String)>,
}

impl fmt::Debug for ProvedorOAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProvedorOAuth")
            .field("endereco_de_autorizacao", &self.endereco_de_autorizacao)
            .field("endereco_de_token", &self.endereco_de_token)
            .field("id_do_cliente", &self.id_do_cliente)
            .field("segredo_do_cliente", &self.segredo_do_cliente.is_some())
            .field("escopos", &self.escopos)
            .finish_non_exhaustive()
    }
}

impl ProvedorOAuth {
    /// Provedor com os dois endpoints (HTTPS, sem consulta) e o id do cliente.
    ///
    /// # Errors
    /// [`ErroOAuth::Local`] se algum endpoint não é HTTPS bem formado.
    pub fn novo(
        endereco_de_autorizacao: &str,
        endereco_de_token: &str,
        id_do_cliente: impl Into<String>,
    ) -> Result<Self, ErroOAuth> {
        if ler_https(endereco_de_autorizacao).is_none() || ler_https(endereco_de_token).is_none() {
            return Err(ErroOAuth::Local {
                detalhe: "endpoint do provedor não é HTTPS",
            });
        }
        Ok(Self {
            endereco_de_autorizacao: endereco_de_autorizacao.to_owned(),
            endereco_de_token: endereco_de_token.to_owned(),
            id_do_cliente: id_do_cliente.into(),
            segredo_do_cliente: None,
            escopos: Vec::new(),
            extras: Vec::new(),
        })
    }

    /// O Google, com os escopos `openid email` (o `id_token` traz o e-mail verificado).
    pub fn google(id_do_cliente: impl Into<String>) -> Self {
        Self {
            endereco_de_autorizacao: GOOGLE_AUTORIZACAO.to_owned(),
            endereco_de_token: GOOGLE_TOKEN.to_owned(),
            id_do_cliente: id_do_cliente.into(),
            segredo_do_cliente: None,
            escopos: vec!["openid".to_owned(), "email".to_owned()],
            extras: vec![("prompt".to_owned(), "select_account".to_owned())],
        }
    }

    /// O "segredo" do cliente de app nativo, quando o provedor o exige (o Google exige para
    /// clientes do tipo app de computador, sem tratá-lo como segredo de verdade).
    pub fn segredo_do_cliente(mut self, segredo: Segredo) -> Self {
        self.segredo_do_cliente = Some(segredo);
        self
    }

    pub fn escopos(mut self, escopos: &[&str]) -> Self {
        self.escopos = escopos.iter().map(|e| (*e).to_owned()).collect();
        self
    }

    /// Parâmetro extra na página de autorização (`prompt`, `login_hint`…).
    pub fn parametro(mut self, chave: impl Into<String>, valor: impl Into<String>) -> Self {
        self.extras.push((chave.into(), valor.into()));
        self
    }

    /// O endereço do catálogo para o endpoint de token (o portão só fala com o catálogo).
    ///
    /// # Errors
    /// Endpoint que não é HTTPS.
    pub fn endereco_do_token(&self) -> Result<Endereco, ErroDeRede> {
        Endereco::novo("token OAuth", &self.endereco_de_token)
    }

    pub fn endereco_de_autorizacao(&self) -> &str {
        &self.endereco_de_autorizacao
    }
}

/// Uma autorização em andamento: a porta de loopback aberta, o verificador PKCE e o `state`.
pub struct FluxoOAuth {
    provedor: ProvedorOAuth,
    retorno: RetornoPorLoopback,
    verificador: Segredo,
    estado: Segredo,
    endereco: String,
}

impl fmt::Debug for FluxoOAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FluxoOAuth")
            .field("provedor", &self.provedor)
            .field("retorno", &self.retorno)
            .finish_non_exhaustive()
    }
}

fn aleatorio_base64(bytes: usize) -> Result<Segredo, ErroOAuth> {
    let mut buf = Zeroizing::new(vec![0u8; bytes]);
    getrandom::fill(buf.as_mut_slice()).map_err(|_| ErroOAuth::Local {
        detalhe: "sem gerador aleatório do sistema",
    })?;
    Ok(Segredo::novo(URL_SAFE_NO_PAD.encode(buf.as_slice())))
}

/// O desafio PKCE S256 de um verificador: `base64url(sha256(verificador))`.
pub fn desafio_pkce(verificador: &Segredo) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verificador.expor().as_bytes()))
}

/// Comparação em tempo que não depende de onde os textos diferem.
fn iguais(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

fn sanitizar_erro(erro: &str) -> String {
    erro.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .take(64)
        .collect()
}

impl FluxoOAuth {
    /// Abre a porta de loopback e monta a página de autorização.
    ///
    /// # Errors
    /// Porta indisponível ou sem gerador aleatório.
    pub fn iniciar(
        provedor: ProvedorOAuth,
        caminho_do_retorno: &str,
        nome_do_app: &str,
    ) -> Result<Self, ErroOAuth> {
        let retorno = RetornoPorLoopback::escutar(caminho_do_retorno, nome_do_app)
            .map_err(ErroOAuth::Retorno)?;
        let verificador = aleatorio_base64(32)?;
        let estado = aleatorio_base64(24)?;
        let mut pares: Vec<(&str, String)> = vec![
            ("response_type", "code".to_owned()),
            ("client_id", provedor.id_do_cliente.clone()),
            ("redirect_uri", retorno.endereco_de_retorno()),
            ("code_challenge_method", "S256".to_owned()),
            ("code_challenge", desafio_pkce(&verificador)),
            ("scope", provedor.escopos.join(" ")),
            ("state", estado.expor().to_owned()),
        ];
        for (k, v) in &provedor.extras {
            pares.push((k.as_str(), v.clone()));
        }
        let consulta = pares
            .iter()
            .map(|(k, v)| format!("{}={}", codificar(k), codificar(v)))
            .collect::<Vec<_>>()
            .join("&");
        let endereco = format!("{}?{consulta}", provedor.endereco_de_autorizacao);
        Ok(Self {
            provedor,
            retorno,
            verificador,
            estado,
            endereco,
        })
    }

    /// A página de autorização, para o navegador do sistema.
    pub fn endereco_de_autorizacao(&self) -> &str {
        &self.endereco
    }

    /// O `redirect_uri` (loopback) deste fluxo.
    pub fn endereco_de_retorno(&self) -> String {
        self.retorno.endereco_de_retorno()
    }

    /// Pede ao sistema que abra a página de autorização. Só abre se o endereço é exatamente o
    /// que este fluxo montou, sobre o endpoint de autorização do provedor.
    pub fn abrir_no_navegador(&self) -> bool {
        self.endereco
            .starts_with(&format!("{}?", self.provedor.endereco_de_autorizacao))
            && abrir_no_navegador(&self.endereco)
    }

    /// Espera o retorno do navegador e confere o `state`. Bloqueia: fora da thread da
    /// interface.
    ///
    /// # Errors
    /// Prazo, cancelamento, `state` que não confere, autorização negada ou retorno malformado.
    pub fn esperar(
        self,
        relogio: &dyn Relogio,
        prazo: Instante,
        cancelar: &AtomicBool,
    ) -> Result<CodigoDeAutorizacao, ErroOAuth> {
        let endereco_de_retorno = self.retorno.endereco_de_retorno();
        let parametros = self
            .retorno
            .esperar(relogio, prazo, cancelar)
            .map_err(ErroOAuth::Retorno)?;
        let estado_ok = parametros.quantas("state") == 1
            && parametros
                .valor("state")
                .is_some_and(|s| iguais(s, self.estado.expor()));
        if !estado_ok {
            return Err(ErroOAuth::EstadoNaoConfere);
        }
        if let Some(erro) = parametros.valor("error") {
            return Err(ErroOAuth::Negada {
                erro: sanitizar_erro(erro),
            });
        }
        let codigo = match parametros.valor("code") {
            Some(c)
                if parametros.quantas("code") == 1
                    && !c.is_empty()
                    && c.len() <= CODIGO_MAXIMO
                    && c.bytes().all(|b| b.is_ascii_graphic()) =>
            {
                Segredo::novo(c)
            }
            _ => return Err(ErroOAuth::RetornoMalformado),
        };
        Ok(CodigoDeAutorizacao {
            provedor: self.provedor,
            codigo,
            verificador: self.verificador,
            endereco_de_retorno,
        })
    }
}

/// O código recebido pelo loopback, pronto para a troca.
pub struct CodigoDeAutorizacao {
    provedor: ProvedorOAuth,
    codigo: Segredo,
    verificador: Segredo,
    endereco_de_retorno: String,
}

impl fmt::Debug for CodigoDeAutorizacao {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CodigoDeAutorizacao(***)")
    }
}

#[derive(Deserialize)]
struct RespostaDoToken {
    id_token: Option<String>,
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
}

/// Os tokens devolvidos pelo provedor. O `Debug` diz só quais vieram.
pub struct TokensOAuth {
    pub id_token: Option<Segredo>,
    pub token_de_acesso: Option<Segredo>,
    pub token_de_renovacao: Option<Segredo>,
    pub expira_em_segundos: Option<u64>,
}

impl fmt::Debug for TokensOAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TokensOAuth")
            .field("id_token", &self.id_token.is_some())
            .field("token_de_acesso", &self.token_de_acesso.is_some())
            .field("token_de_renovacao", &self.token_de_renovacao.is_some())
            .field("expira_em_segundos", &self.expira_em_segundos)
            .finish()
    }
}

impl CodigoDeAutorizacao {
    /// Troca o código pelos tokens no endpoint de token, pelo portão (o endpoint precisa
    /// estar no catálogo: [`ProvedorOAuth::endereco_do_token`]). Bloqueia.
    ///
    /// # Errors
    /// Código recusado, resposta fora do formato ou falha do portão.
    pub fn trocar(self, rede: &Rede) -> Result<TokensOAuth, ErroOAuth> {
        let mut pares: Vec<(&str, &str)> = vec![
            ("grant_type", "authorization_code"),
            ("code", self.codigo.expor()),
            ("redirect_uri", &self.endereco_de_retorno),
            ("client_id", &self.provedor.id_do_cliente),
            ("code_verifier", self.verificador.expor()),
        ];
        if let Some(segredo) = &self.provedor.segredo_do_cliente {
            pares.push(("client_secret", segredo.expor()));
        }
        let pedido = Pedido::post(self.provedor.endereco_de_token.clone())
            .formulario(&pares)
            .aceitar("application/json")
            .prazo(PRAZO_DA_TROCA);
        let resposta = match rede.executar(&pedido) {
            Ok(r) => r,
            Err(ErroDeRede::Http { status, .. }) if (400..500).contains(&status) => {
                return Err(ErroOAuth::CodigoRecusado);
            }
            Err(e) => return Err(ErroOAuth::Rede(e)),
        };
        let corpo = Zeroizing::new(resposta.corpo);
        let lida: RespostaDoToken =
            serde_json::from_slice(&corpo).map_err(|_| ErroOAuth::RespostaMalformada)?;
        let tokens = TokensOAuth {
            id_token: lida.id_token.map(Segredo::novo),
            token_de_acesso: lida.access_token.map(Segredo::novo),
            token_de_renovacao: lida.refresh_token.map(Segredo::novo),
            expira_em_segundos: lida.expires_in,
        };
        if tokens.id_token.is_none() && tokens.token_de_acesso.is_none() {
            return Err(ErroOAuth::RespostaMalformada);
        }
        Ok(tokens)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn o_desafio_pkce_e_base64url_do_sha256() {
        // Valor de conferência calculado fora do Rust:
        // `printf '<verificador>' | openssl dgst -sha256 -binary | base64` em base64url.
        let verificador = Segredo::novo("dBjftJeZ4CVP-mJ92K1R8XXL2aS54W3kV7c6e4i7qJk");
        assert_eq!(
            desafio_pkce(&verificador),
            "Gmxpo92nurYlhs6ynVEqSzc3hTagGS03KBV3DhzY81U"
        );
    }

    #[test]
    fn a_pagina_de_autorizacao_tem_pkce_estado_e_loopback() {
        let fluxo = FluxoOAuth::iniciar(ProvedorOAuth::google("cliente-teste"), "/retorno", "App")
            .expect("fluxo");
        let url = fluxo.endereco_de_autorizacao();
        assert!(url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A"));
        assert!(url.contains("scope=openid%20email"));
        assert!(url.contains("state="));
        assert!(
            !url.contains(fluxo.verificador.expor()),
            "o verificador não vai na URL"
        );
    }

    #[test]
    fn endpoint_sem_https_e_recusado() {
        assert!(ProvedorOAuth::novo("http://a.b/auth", "https://a.b/token", "x").is_err());
    }

    #[test]
    fn o_erro_do_provedor_e_sanitizado() {
        assert_eq!(
            sanitizar_erro("access_denied<script>"),
            "access_deniedscript"
        );
    }
}
