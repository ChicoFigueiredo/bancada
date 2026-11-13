//! O catálogo: os únicos endereços HTTPS com que o app fala. Quem monta é o app (o endereço
//! do servidor vem da configuração), mas nenhum pedido sai para fora dele, e é o catálogo, não
//! o pedido, que diz se o endereço recebe a credencial.

use crate::erro::ErroDeRede;
use crate::pedido::{Alvo, ler_https, sanitizado};

/// Um endereço permitido: host (e porta, se não for 443) mais um prefixo de caminho.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endereco {
    nome: String,
    host: String,
    porta: Option<u16>,
    prefixo: String,
    recebe_autorizacao: bool,
}

impl Endereco {
    /// `base` é uma URL HTTPS sem consulta (`https://sbf.exemplo.com.br/api/`). Todo pedido
    /// cujo caminho começa pelo caminho de `base` (num limite de segmento) cai neste endereço.
    pub fn novo(nome: impl Into<String>, base: &str) -> Result<Self, ErroDeRede> {
        let alvo = ler_https(base).ok_or_else(|| ErroDeRede::EnderecoForaDoCatalogo {
            endereco: sanitizado(base),
        })?;
        Ok(Self {
            nome: nome.into(),
            host: alvo.host,
            porta: alvo.porta,
            prefixo: alvo.caminho,
            recebe_autorizacao: false,
        })
    }

    /// Este endereço recebe o cabeçalho `Authorization` da fonte de autorização.
    pub fn com_autorizacao(mut self) -> Self {
        self.recebe_autorizacao = true;
        self
    }

    pub fn nome(&self) -> &str {
        &self.nome
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn porta(&self) -> Option<u16> {
        self.porta
    }

    pub fn prefixo(&self) -> &str {
        &self.prefixo
    }

    pub fn recebe_autorizacao(&self) -> bool {
        self.recebe_autorizacao
    }

    /// `host[:porta]`, como a rota dos testes e o estado da rede o chamam.
    pub fn autoridade(&self) -> String {
        match self.porta {
            Some(porta) => format!("{}:{porta}", self.host),
            None => self.host.clone(),
        }
    }

    fn cobre(&self, alvo: &Alvo) -> bool {
        if self.host != alvo.host || self.porta != alvo.porta {
            return false;
        }
        let Some(resto) = alvo.caminho.strip_prefix(&self.prefixo) else {
            return false;
        };
        self.prefixo.ends_with('/') || resto.is_empty() || resto.starts_with('/')
    }
}

/// A lista de endereços permitidos.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Catalogo {
    enderecos: Vec<Endereco>,
}

impl Catalogo {
    pub fn novo() -> Self {
        Self::default()
    }

    pub fn com(mut self, endereco: Endereco) -> Self {
        self.enderecos.push(endereco);
        self
    }

    pub fn enderecos(&self) -> &[Endereco] {
        &self.enderecos
    }

    /// O endereço que cobre `url` (o de prefixo mais longo), se houver.
    pub fn endereco_de(&self, url: &str) -> Option<&Endereco> {
        ler_https(url).and_then(|alvo| self.endereco_para(&alvo))
    }

    pub(crate) fn endereco_para(&self, alvo: &Alvo) -> Option<&Endereco> {
        self.enderecos
            .iter()
            .filter(|e| e.cobre(alvo))
            .max_by_key(|e| e.prefixo.len())
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn catalogo() -> Catalogo {
        Catalogo::novo()
            .com(
                Endereco::novo("api", "https://sbf.exemplo.com.br/api")
                    .expect("endereço")
                    .com_autorizacao(),
            )
            .com(Endereco::novo("token", "https://oauth.exemplo.com/token").expect("endereço"))
    }

    #[test]
    fn o_prefixo_respeita_o_limite_de_segmento() {
        let c = catalogo();
        assert_eq!(
            c.endereco_de("https://sbf.exemplo.com.br/api/v1/estado")
                .map(Endereco::nome),
            Some("api")
        );
        assert_eq!(
            c.endereco_de("https://sbf.exemplo.com.br/api")
                .map(Endereco::nome),
            Some("api")
        );
        assert!(c.endereco_de("https://sbf.exemplo.com.br/apix").is_none());
        assert!(c.endereco_de("https://sbf.exemplo.com.br/").is_none());
        assert!(c.endereco_de("https://outro.exemplo.com.br/api").is_none());
        assert!(
            c.endereco_de("https://sbf.exemplo.com.br:8443/api")
                .is_none()
        );
        assert!(c.endereco_de("http://sbf.exemplo.com.br/api").is_none());
    }

    #[test]
    fn so_https_entra_no_catalogo() {
        assert!(Endereco::novo("x", "http://sbf.exemplo.com.br/api").is_err());
        assert!(Endereco::novo("x", "https://sbf.exemplo.com.br/api?a=1").is_err());
    }
}
