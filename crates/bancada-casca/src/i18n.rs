//! Textos com Fluent. A casca traz os próprios em `i18n/<idioma>/casca.ftl`, embutidos no
//! binário (o app é offline e a tradução nunca pode faltar). O app acrescenta os dele com
//! [`Textos::acrescentar`] e pode usar o mesmo `Textos` para os rótulos das Ações.

use fluent_bundle::{FluentArgs, FluentBundle, FluentResource, FluentValue};
use std::fmt;
use unic_langid::LanguageIdentifier;

const CASCA_PT_BR: &str = include_str!("../i18n/pt-BR/casca.ftl");

/// Por que um recurso Fluent foi recusado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErroTextos(pub String);

impl fmt::Display for ErroTextos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "recurso Fluent inválido: {}", self.0)
    }
}

impl std::error::Error for ErroTextos {}

/// Os textos de um idioma.
pub struct Textos {
    pacote: FluentBundle<FluentResource>,
}

impl fmt::Debug for Textos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Textos")
            .field("idiomas", &self.pacote.locales)
            .finish_non_exhaustive()
    }
}

impl Default for Textos {
    fn default() -> Self {
        Self::pt_br()
    }
}

impl Textos {
    /// pt-BR com os textos da casca.
    pub fn pt_br() -> Self {
        let idioma: LanguageIdentifier = unic_langid::langid!("pt-BR");
        let mut pacote = FluentBundle::new(vec![idioma]);
        // Sem os caracteres de isolamento bidirecional: a interface é só da esquerda para a
        // direita, e eles apareceriam como quadradinhos em algumas fontes.
        pacote.set_use_isolating(false);
        let mut textos = Self { pacote };
        // O recurso embutido é conferido pelos testes; erro aqui é de build.
        if let Err(erro) = textos.acrescentar(CASCA_PT_BR) {
            debug_assert!(false, "casca.ftl inválido: {erro}");
        }
        textos
    }

    /// Acrescenta um recurso `.ftl` do app. Uma mensagem com id já existente é recusada (os ids
    /// da casca começam por `casca-`).
    pub fn acrescentar(&mut self, ftl: &str) -> Result<(), ErroTextos> {
        let recurso = FluentResource::try_new(ftl.to_owned())
            .map_err(|(_, erros)| ErroTextos(format!("{erros:?}")))?;
        self.pacote
            .add_resource(recurso)
            .map_err(|erros| ErroTextos(format!("{erros:?}")))
    }

    pub fn tem(&self, id: &str) -> bool {
        self.pacote.has_message(id)
    }

    /// O texto da mensagem; o próprio id se faltar (visível, para ninguém deixar passar).
    pub fn t(&self, id: &str) -> String {
        self.formatar(id, None)
    }

    /// O texto com argumentos (`{ $nome }`).
    pub fn t_com(&self, id: &str, args: &[(&str, &str)]) -> String {
        let mut fa = FluentArgs::new();
        for (k, v) in args {
            fa.set(*k, FluentValue::from(*v));
        }
        self.formatar(id, Some(&fa))
    }

    fn formatar(&self, id: &str, args: Option<&FluentArgs>) -> String {
        let Some(msg) = self.pacote.get_message(id) else {
            return id.to_owned();
        };
        let Some(padrao) = msg.value() else {
            return id.to_owned();
        };
        let mut erros = Vec::new();
        self.pacote
            .format_pattern(padrao, args, &mut erros)
            .into_owned()
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn textos_da_casca_em_pt_br() {
        let t = Textos::pt_br();
        assert_eq!(t.t("casca-voltar"), "Voltar");
        assert_eq!(
            t.t_com("casca-painel", &[("nome", "Detalhe")]),
            "Painel Detalhe"
        );
        assert_eq!(t.t("nao-existe"), "nao-existe");
    }

    #[test]
    fn o_app_acrescenta_os_dele_sem_sobrescrever() {
        let mut t = Textos::pt_br();
        t.acrescentar("sbf-triagem = Triagem").unwrap();
        assert_eq!(t.t("sbf-triagem"), "Triagem");
        assert!(t.acrescentar("casca-voltar = Outro").is_err());
        assert_eq!(t.t("casca-voltar"), "Voltar");
        assert!(t.acrescentar("isto não é fluent {").is_err());
    }
}
