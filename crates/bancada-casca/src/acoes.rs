//! A lista única de Ações: tudo o que o usuário pode mandar o app fazer. Ribbon, Barra de
//! Acesso Rápido, KeyTips, Paleta e Mapa de atalhos leem esta mesma lista, que o app fornece
//! a cada quadro (o estado de cada Ação, habilitada ou desabilitada com motivo, muda com o app).
//!
//! A casca acrescenta as próprias Ações (Voltar, Avançar, Paleta, modos do Ribbon, painéis do
//! dock), todas com id começando por `casca.`; o app as posiciona nas abas como qualquer outra.

use crate::atalhos::Atalho;
use std::collections::{HashMap, HashSet};
use std::fmt;

/// Identificador estável de uma Ação (`"triagem.descartar"`). Nunca muda entre versões.
pub type IdAcao = String;

/// Como a Ação se comporta na interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TipoAcao {
    /// Executa e pronto.
    #[default]
    Comando,
    /// Liga e desliga: aparece marcada ou não.
    Alternancia,
}

/// Estado de uma Ação neste quadro.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Disponibilidade {
    #[default]
    Habilitada,
    /// Desabilitada, com o motivo mostrado na dica ("Chega na fatia 3, com a caixa de entrada").
    Desabilitada { motivo: String },
}

/// Uma Ação da lista única.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Acao {
    pub id: IdAcao,
    /// Rótulo já traduzido ("Descartar").
    pub rotulo: String,
    /// Nome do ícone na família de ícones do app (`"delete"`); vazio = sem ícone.
    pub icone: String,
    pub atalho: Option<Atalho>,
    /// Dica do tooltip e da Paleta.
    pub dica: String,
    /// Letras do KeyTip, estáveis e únicas dentro da aba (`"D"`, `"DS"`). Vazio = sem KeyTip.
    pub keytip: String,
    pub tipo: TipoAcao,
    pub disponibilidade: Disponibilidade,
    /// Só para `Alternancia`: ligada agora.
    pub marcada: bool,
    /// Fica fora da Paleta (por exemplo a própria Paleta).
    pub fora_da_paleta: bool,
    /// Ação com menu (por exemplo "Estado do achado ⏷"): executar abre o menu com estas Ações
    /// da lista, em vez de chegar ao app.
    pub menu: Vec<IdAcao>,
}

impl Acao {
    /// Ação de comando, habilitada, sem atalho nem ícone.
    pub fn nova(id: impl Into<String>, rotulo: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            rotulo: rotulo.into(),
            icone: String::new(),
            atalho: None,
            dica: String::new(),
            keytip: String::new(),
            tipo: TipoAcao::Comando,
            disponibilidade: Disponibilidade::Habilitada,
            marcada: false,
            fora_da_paleta: false,
            menu: Vec::new(),
        }
    }

    /// Faz da Ação um menu com as Ações dadas (ids da lista única).
    pub fn menu(mut self, ids: &[&str]) -> Self {
        self.menu = ids.iter().map(|i| (*i).to_owned()).collect();
        self
    }

    pub fn icone(mut self, nome: impl Into<String>) -> Self {
        self.icone = nome.into();
        self
    }

    pub fn atalho(mut self, atalho: Atalho) -> Self {
        self.atalho = Some(atalho);
        self
    }

    pub fn dica(mut self, dica: impl Into<String>) -> Self {
        self.dica = dica.into();
        self
    }

    pub fn keytip(mut self, letras: impl Into<String>) -> Self {
        self.keytip = letras.into().to_ascii_uppercase();
        self
    }

    /// Torna a Ação uma alternância, ligada ou não.
    pub fn alternancia(mut self, marcada: bool) -> Self {
        self.tipo = TipoAcao::Alternancia;
        self.marcada = marcada;
        self
    }

    pub fn desabilitada(mut self, motivo: impl Into<String>) -> Self {
        self.disponibilidade = Disponibilidade::Desabilitada {
            motivo: motivo.into(),
        };
        self
    }

    /// Habilitada se `condicao`, senão desabilitada com `motivo`.
    pub fn habilitada_se(self, condicao: bool, motivo: impl Into<String>) -> Self {
        if condicao {
            self
        } else {
            self.desabilitada(motivo)
        }
    }

    pub fn fora_da_paleta(mut self) -> Self {
        self.fora_da_paleta = true;
        self
    }

    pub fn esta_habilitada(&self) -> bool {
        self.disponibilidade == Disponibilidade::Habilitada
    }

    pub fn motivo(&self) -> Option<&str> {
        match &self.disponibilidade {
            Disponibilidade::Habilitada => None,
            Disponibilidade::Desabilitada { motivo } => Some(motivo),
        }
    }

    /// Texto do tooltip: a dica, o atalho e, se desabilitada, o motivo.
    pub fn texto_da_dica(&self) -> String {
        let mut texto = if self.dica.is_empty() {
            self.rotulo.clone()
        } else {
            self.dica.clone()
        };
        if let Some(atalho) = &self.atalho {
            texto.push_str(&format!(" ({atalho})"));
        }
        if let Some(motivo) = self.motivo() {
            texto.push_str("\n\n");
            texto.push_str(motivo);
        }
        texto
    }
}

/// Por que uma lista de Ações não é válida.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroAcoes {
    IdRepetido(IdAcao),
    IdVazio,
    /// Duas Ações com o mesmo atalho.
    AtalhoEmConflito {
        atalho: Atalho,
        primeira: IdAcao,
        segunda: IdAcao,
    },
}

impl fmt::Display for ErroAcoes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErroAcoes::IdRepetido(id) => write!(f, "Ação repetida: {id}"),
            ErroAcoes::IdVazio => write!(f, "Ação sem id"),
            ErroAcoes::AtalhoEmConflito {
                atalho,
                primeira,
                segunda,
            } => write!(f, "o atalho {atalho} está em {primeira} e em {segunda}"),
        }
    }
}

impl std::error::Error for ErroAcoes {}

/// A lista única, já validada: ids únicos e atalhos sem conflito.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListaDeAcoes {
    acoes: Vec<Acao>,
    indice: HashMap<IdAcao, usize>,
}

impl ListaDeAcoes {
    pub fn nova(acoes: Vec<Acao>) -> Result<Self, ErroAcoes> {
        let mut indice = HashMap::new();
        let mut atalhos: HashMap<Atalho, &str> = HashMap::new();
        for (i, acao) in acoes.iter().enumerate() {
            if acao.id.is_empty() {
                return Err(ErroAcoes::IdVazio);
            }
            if indice.insert(acao.id.clone(), i).is_some() {
                return Err(ErroAcoes::IdRepetido(acao.id.clone()));
            }
            if let Some(atalho) = acao.atalho
                && let Some(primeira) = atalhos.insert(atalho, &acao.id)
            {
                return Err(ErroAcoes::AtalhoEmConflito {
                    atalho,
                    primeira: primeira.to_owned(),
                    segunda: acao.id.clone(),
                });
            }
        }
        Ok(Self { acoes, indice })
    }

    pub fn get(&self, id: &str) -> Option<&Acao> {
        self.indice.get(id).map(|&i| &self.acoes[i])
    }

    pub fn iter(&self) -> impl Iterator<Item = &Acao> {
        self.acoes.iter()
    }

    pub fn len(&self) -> usize {
        self.acoes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.acoes.is_empty()
    }

    pub fn esta_habilitada(&self, id: &str) -> bool {
        self.get(id).is_some_and(Acao::esta_habilitada)
    }

    /// A Ação cujo atalho bate com a tecla (modificadores exatos).
    pub fn por_atalho(&self, tecla: egui::Key, mods: egui::Modifiers) -> Option<&Acao> {
        self.acoes
            .iter()
            .find(|a| a.atalho.is_some_and(|t| t.bate(tecla, mods)))
    }

    /// O Mapa de atalhos: (atalho, rótulo, id), na ordem da lista.
    pub fn mapa_de_atalhos(&self) -> Vec<(Atalho, &str, &str)> {
        self.acoes
            .iter()
            .filter_map(|a| a.atalho.map(|t| (t, a.rotulo.as_str(), a.id.as_str())))
            .collect()
    }

    /// Ids que aparecem mais de uma vez num conjunto de referências (útil para validar abas).
    pub fn faltando<'a>(&self, ids: impl IntoIterator<Item = &'a str>) -> Vec<String> {
        let mut vistos = HashSet::new();
        ids.into_iter()
            .filter(|id| !self.indice.contains_key(*id) && vistos.insert(*id))
            .map(str::to_owned)
            .collect()
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use egui::Key;

    #[test]
    fn ids_repetidos_sao_recusados() {
        let erro = ListaDeAcoes::nova(vec![Acao::nova("a", "A"), Acao::nova("a", "B")]);
        assert_eq!(erro, Err(ErroAcoes::IdRepetido("a".into())));
    }

    #[test]
    fn atalho_em_conflito_e_recusado_com_as_duas_acoes() {
        let erro = ListaDeAcoes::nova(vec![
            Acao::nova("a", "A").atalho(Atalho::ctrl(Key::K)),
            Acao::nova("b", "B").atalho(Atalho::ctrl(Key::K)),
        ]);
        assert!(matches!(
            erro,
            Err(ErroAcoes::AtalhoEmConflito { ref primeira, ref segunda, .. })
                if primeira == "a" && segunda == "b"
        ));
        // O mesmo tecla com outro modificador não é conflito.
        assert!(
            ListaDeAcoes::nova(vec![
                Acao::nova("a", "A").atalho(Atalho::ctrl(Key::K)),
                Acao::nova("b", "B").atalho(Atalho::ctrl(Key::K).com_shift()),
            ])
            .is_ok()
        );
    }

    #[test]
    fn dica_mostra_atalho_e_motivo() {
        let a = Acao::nova("lote.enviar", "Enviar ao servidor")
            .atalho(Atalho::ctrl(Key::E))
            .desabilitada("Chega na fatia 3, com a caixa de entrada");
        assert!(!a.esta_habilitada());
        let dica = a.texto_da_dica();
        assert!(dica.contains("Ctrl+E"));
        assert!(dica.contains("Chega na fatia 3"));
    }

    #[test]
    fn busca_por_atalho_e_exata() {
        let lista = ListaDeAcoes::nova(vec![
            Acao::nova("voltar", "Voltar").atalho(Atalho::alt(Key::ArrowLeft)),
        ])
        .unwrap();
        assert!(
            lista
                .por_atalho(Key::ArrowLeft, egui::Modifiers::ALT)
                .is_some()
        );
        assert!(
            lista
                .por_atalho(Key::ArrowLeft, egui::Modifiers::NONE)
                .is_none()
        );
    }
}
