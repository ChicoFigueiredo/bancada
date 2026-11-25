//! A política que o app injeta no navegador: o que não se clica, quais URLs não saem e qual
//! script de ponte se instala na página. A **lista** de termos é do app (no SBF, a lista de
//! bloqueio); o motor só pergunta.

use std::fmt;

/// Um elemento descrito para a conferência: cada campo é examinado sozinho.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Candidato {
    /// Pares (nome do campo, valor): `texto`, `aria-label`, `value`, `name`, `id`, `title`,
    /// `alt`, `destino`. Nunca leva o valor de um campo de senha.
    pub campos: Vec<(String, String)>,
    /// O elemento é um campo de senha (o executor não deixa preenchê-lo com texto literal).
    pub eh_senha: bool,
    /// O elemento é um `input`/`textarea`/`select` (para os passos de preencher).
    pub eh_campo: bool,
    /// O tipo do campo, como o **navegador** o entende (`password`, `text`, `tel`…; a tag em
    /// minúsculas para o que não é `input`). Vazio quando não se sabe.
    pub tipo: String,
}

impl Candidato {
    /// Um candidato só com alguns campos, para testes e para o executor.
    #[must_use]
    pub fn com(campos: &[(&str, &str)]) -> Self {
        Self {
            campos: campos
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
            ..Self::default()
        }
    }
}

/// Por que um elemento foi barrado. O `trecho` é só o termo e o campo, nunca o texto todo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bloqueio {
    /// O campo que casou (`texto`, `destino`…).
    pub campo: String,
    /// O termo da lista que casou.
    pub termo: String,
}

impl fmt::Display for Bloqueio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.campo == "rede" {
            return write!(
                f,
                "uma requisição de rede foi barrada (\"{}\"), e roteiro só consulta e baixa",
                self.termo
            );
        }
        write!(
            f,
            "o campo \"{}\" do elemento indica \"{}\", e roteiro só consulta e baixa",
            self.campo, self.termo
        )
    }
}

/// O que o Chrome/CDP pergunta ao app.
pub trait Politica {
    /// Confere um elemento. `Some` se algum campo casa com a lista de bloqueio do app.
    fn avaliar(&self, candidato: &Candidato) -> Option<Bloqueio>;
    /// Confere uma URL (navegar, destino de link, requisição de rede).
    fn avaliar_url(&self, url: &str) -> Option<Bloqueio>;
    /// O script da ponte instalado em todo documento novo, já com a lista do app no lugar.
    /// O motor oferece o texto-base em [`crate::chrome::SCRIPT_DA_PONTE`], que traz o marcador
    /// [`crate::chrome::MARCADOR_DA_LISTA`] onde a lista em JSON entra.
    fn script_da_ponte(&self) -> String;
}
