//! Família de ícones plugável. O app registra a fonte (opcional) e o mapa nome → glifo; a
//! casca desenha os ícones por nome. O SBF usa Material Symbols Rounded, o Chess usa Phosphor;
//! a casca não embute nenhuma das duas. Sem glifo para um nome, o botão mostra só o rótulo.

use egui::{FontData, FontDefinitions, FontFamily, FontId, RichText};
use std::collections::HashMap;
use std::sync::Arc;

/// Nome da família do egui onde a fonte de ícones do app é registrada.
pub const FAMILIA_ICONES: &str = "bancada-icones";

/// A família de ícones de um app.
#[derive(Clone, Default)]
pub struct FamiliaDeIcones {
    /// Bytes da fonte (TTF/OTF). `None` = os glifos vêm das fontes padrão do egui.
    pub fonte: Option<Arc<Vec<u8>>>,
    glifos: HashMap<String, String>,
}

impl std::fmt::Debug for FamiliaDeIcones {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FamiliaDeIcones")
            .field("com_fonte", &self.fonte.is_some())
            .field("glifos", &self.glifos.len())
            .finish()
    }
}

impl FamiliaDeIcones {
    /// Família sem fonte própria: os glifos precisam existir nas fontes padrão do egui.
    pub fn sem_fonte() -> Self {
        Self::default()
    }

    /// Família com a fonte do app (por exemplo `MaterialSymbolsRounded.ttf`).
    pub fn com_fonte(bytes: Vec<u8>) -> Self {
        Self {
            fonte: Some(Arc::new(bytes)),
            glifos: HashMap::new(),
        }
    }

    /// Registra um glifo: `"delete"` → `"\u{e872}"`.
    pub fn glifo(mut self, nome: impl Into<String>, glifo: impl Into<String>) -> Self {
        self.glifos.insert(nome.into(), glifo.into());
        self
    }

    /// Registra vários de uma vez.
    pub fn glifos<'a>(mut self, pares: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        for (nome, glifo) in pares {
            self.glifos.insert(nome.to_owned(), glifo.to_owned());
        }
        self
    }

    pub fn get(&self, nome: &str) -> Option<&str> {
        if nome.is_empty() {
            return None;
        }
        self.glifos.get(nome).map(String::as_str)
    }

    /// A família do egui a usar para desenhar os glifos.
    pub fn familia(&self) -> FontFamily {
        if self.fonte.is_some() {
            FontFamily::Name(FAMILIA_ICONES.into())
        } else {
            FontFamily::Proportional
        }
    }

    /// O ícone pronto para um `Label`/`Button`, no tamanho dado; `None` se não há glifo.
    pub fn texto(&self, nome: &str, tamanho: f32) -> Option<RichText> {
        self.get(nome)
            .map(|g| RichText::new(g).font(FontId::new(tamanho, self.familia())))
    }

    /// Acrescenta a fonte às definições do egui: família própria e reserva da proporcional
    /// (para um glifo no meio de um texto também aparecer).
    pub fn instalar(&self, fontes: &mut FontDefinitions) {
        let Some(bytes) = &self.fonte else {
            return;
        };
        fontes.font_data.insert(
            FAMILIA_ICONES.to_owned(),
            Arc::new(FontData::from_owned(bytes.as_ref().clone())),
        );
        let mut familia = vec![FAMILIA_ICONES.to_owned()];
        if let Some(prop) = fontes.families.get(&FontFamily::Proportional) {
            familia.extend(prop.iter().cloned());
        }
        fontes
            .families
            .insert(FontFamily::Name(FAMILIA_ICONES.into()), familia);
        fontes
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .push(FAMILIA_ICONES.to_owned());
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn mapa_nome_glifo_e_familia() {
        let f = FamiliaDeIcones::sem_fonte().glifo("voltar", "⏴");
        assert_eq!(f.get("voltar"), Some("⏴"));
        assert_eq!(f.get("nada"), None);
        assert_eq!(f.get(""), None);
        assert_eq!(f.familia(), FontFamily::Proportional);
        let mut defs = FontDefinitions::default();
        f.instalar(&mut defs);
        assert!(!defs.font_data.contains_key(FAMILIA_ICONES));
    }
}
