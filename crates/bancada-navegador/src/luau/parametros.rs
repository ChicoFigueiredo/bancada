//! Os parâmetros do roteiro: valores JÁ RESOLVIDOS pelo app (mês relativo vira "AAAA-MM")
//! entregues ao Luau na tabela global `parametros`, somente leitura. Nunca carregam segredo:
//! só texto de mês e sim/não, quem decide o que cabe é o app (manifesto).

use std::collections::BTreeMap;

/// Valor de um parâmetro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValorDeParametro {
    /// Texto (por exemplo, "2026-10").
    Texto(String),
    /// Sim ou não.
    Booleano(bool),
}

/// Mapa nome → valor dos parâmetros de uma execução.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Parametros(BTreeMap<String, ValorDeParametro>);

impl Parametros {
    /// Sem nenhum parâmetro.
    #[must_use]
    pub fn novo() -> Self {
        Self::default()
    }

    /// Põe (ou troca) um parâmetro.
    pub fn inserir(&mut self, nome: impl Into<String>, valor: ValorDeParametro) {
        self.0.insert(nome.into(), valor);
    }

    /// Atalho para um parâmetro de texto.
    pub fn inserir_texto(&mut self, nome: impl Into<String>, texto: impl Into<String>) {
        self.inserir(nome, ValorDeParametro::Texto(texto.into()));
    }

    /// Atalho para um parâmetro sim/não.
    pub fn inserir_booleano(&mut self, nome: impl Into<String>, valor: bool) {
        self.inserir(nome, ValorDeParametro::Booleano(valor));
    }

    /// O valor de um parâmetro.
    #[must_use]
    pub fn obter(&self, nome: &str) -> Option<&ValorDeParametro> {
        self.0.get(nome)
    }

    /// Os parâmetros em ordem de nome.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &ValorDeParametro)> {
        self.0.iter().map(|(n, v)| (n.as_str(), v))
    }

    /// Não há nenhum?
    #[must_use]
    pub fn vazio(&self) -> bool {
        self.0.is_empty()
    }
}
