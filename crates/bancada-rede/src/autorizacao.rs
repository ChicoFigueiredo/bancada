//! A credencial que vai no cabeçalho `Authorization`. Este crate nunca guarda credencial: a
//! cada tentativa pergunta à [`FonteDeAutorizacao`] (o chaveiro do app), e só põe o cabeçalho
//! nos endereços do catálogo que o recebem. Nada aqui aparece em `Debug`, erro ou log.

use crate::catalogo::Endereco;
use crate::erro::CorpoDeErro;
use bancada_segredos::Segredo;
use std::fmt;

/// Tamanho máximo de um token aceito.
pub const TAMANHO_MAXIMO_DO_TOKEN: usize = 4096;

/// O valor do cabeçalho `Authorization`, montado a partir de um [`Segredo`]. Sem `Display`;
/// o `Debug` é `Autorizacao(***)`; a memória é zerada ao descartar.
#[derive(Clone)]
pub struct Autorizacao(Segredo);

impl Autorizacao {
    /// `Authorization: Bearer <token>`. `None` se o token não tem forma de token (vazio, com
    /// espaço ou caractere de controle, longo demais): um cabeçalho assim nunca sai.
    pub fn portador(token: &Segredo) -> Option<Self> {
        let valor = token.expor();
        let forma_de_token = !valor.is_empty()
            && valor.len() <= TAMANHO_MAXIMO_DO_TOKEN
            && valor.bytes().all(|b| b.is_ascii_graphic());
        if !forma_de_token {
            return None;
        }
        // Capacidade exata: sem realocação, nenhuma cópia parcial fica na memória liberada.
        let mut cabecalho = String::with_capacity("Bearer ".len() + valor.len());
        cabecalho.push_str("Bearer ");
        cabecalho.push_str(valor);
        Some(Self(Segredo::novo(cabecalho)))
    }

    pub(crate) fn valor(&self) -> &str {
        self.0.expor()
    }
}

impl fmt::Debug for Autorizacao {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Autorizacao(***)")
    }
}

/// De onde vem o `Authorization` de um endereço: o chaveiro do app implementa este trait.
/// Chamado a cada tentativa, logo antes da rede, para que trocar ou apagar a credencial valha
/// no pedido seguinte.
pub trait FonteDeAutorizacao: Send + Sync + fmt::Debug {
    fn autorizacao(&self, endereco: &Endereco) -> Option<Autorizacao>;

    /// O servidor recusou a credencial (`401` ou `403`), com o corpo da resposta (que pode
    /// dizer o motivo, como "estação revogada"). Chamado antes de o erro voltar a quem pediu.
    fn recusada(&self, _endereco: &Endereco, _status: u16, _corpo: &CorpoDeErro) {}
}

/// Sem credencial nenhuma.
#[derive(Debug, Clone, Copy, Default)]
pub struct SemAutorizacao;

impl FonteDeAutorizacao for SemAutorizacao {
    fn autorizacao(&self, _endereco: &Endereco) -> Option<Autorizacao> {
        None
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn o_debug_nao_mostra_o_token() {
        let autorizacao = Autorizacao::portador(&Segredo::novo("tok-123")).expect("token");
        assert_eq!(format!("{autorizacao:?}"), "Autorizacao(***)");
        assert_eq!(autorizacao.valor(), "Bearer tok-123");
    }

    #[test]
    fn token_sem_forma_de_token_nao_vira_cabecalho() {
        for ruim in ["", "a b", "a\nb", "ção"] {
            assert!(
                Autorizacao::portador(&Segredo::novo(ruim)).is_none(),
                "{ruim:?}"
            );
        }
        let longo = "a".repeat(TAMANHO_MAXIMO_DO_TOKEN + 1);
        assert!(Autorizacao::portador(&Segredo::novo(longo)).is_none());
    }
}
