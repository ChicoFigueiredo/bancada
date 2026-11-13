//! Atalhos de teclado: o texto canônico (`"Ctrl+K"`, `"Alt+Left"`, `"Ctrl+Shift+F1"`), a
//! conversão para o `egui` e a comparação **exata** de modificadores (o `consume_shortcut` do
//! egui ignora Shift e Alt a mais, o que confundiria `Alt+Left` com `Left`).

use egui::{Key, Modifiers};
use std::fmt;

/// Um atalho: modificadores exatos e uma tecla.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Atalho {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub tecla: Key,
}

/// Por que o texto de um atalho não pôde ser lido.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErroAtalho(pub String);

impl fmt::Display for ErroAtalho {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "atalho inválido: {}", self.0)
    }
}

impl std::error::Error for ErroAtalho {}

impl Atalho {
    /// Atalho sem modificadores.
    pub const fn tecla(tecla: Key) -> Self {
        Self {
            ctrl: false,
            alt: false,
            shift: false,
            tecla,
        }
    }

    pub const fn ctrl(tecla: Key) -> Self {
        Self {
            ctrl: true,
            ..Self::tecla(tecla)
        }
    }

    pub const fn alt(tecla: Key) -> Self {
        Self {
            alt: true,
            ..Self::tecla(tecla)
        }
    }

    pub const fn com_shift(mut self) -> Self {
        self.shift = true;
        self
    }

    /// Lê `"Ctrl+Shift+K"`. Os modificadores aceitos são `Ctrl`, `Alt` e `Shift` (sem
    /// distinção de caixa); a tecla usa os nomes do `egui::Key` (`K`, `F1`, `Left`, `Enter`…).
    pub fn ler(texto: &str) -> Result<Self, ErroAtalho> {
        let erro = || ErroAtalho(texto.to_owned());
        let partes: Vec<&str> = texto.split('+').map(str::trim).collect();
        let (tecla, mods) = partes.split_last().ok_or_else(erro)?;
        let mut atalho = Atalho::tecla(Key::from_name(tecla).ok_or_else(erro)?);
        for m in mods {
            match m.to_ascii_lowercase().as_str() {
                "ctrl" | "control" | "cmd" => atalho.ctrl = true,
                "alt" => atalho.alt = true,
                "shift" => atalho.shift = true,
                _ => return Err(erro()),
            }
        }
        Ok(atalho)
    }

    /// Os modificadores do egui que este atalho exige (Ctrl vira `command`, que no macOS é ⌘).
    pub fn modificadores(&self) -> Modifiers {
        Modifiers {
            alt: self.alt,
            ctrl: false,
            shift: self.shift,
            mac_cmd: false,
            command: self.ctrl,
        }
    }

    /// Bate com uma tecla apertada? Modificadores comparados com exatidão.
    pub fn bate(&self, tecla: Key, mods: Modifiers) -> bool {
        tecla == self.tecla
            && mods.command == self.ctrl
            && mods.alt == self.alt
            && mods.shift == self.shift
    }

    /// Tem Ctrl ou Alt (dispara mesmo com um campo de texto em foco).
    pub fn tem_modificador_forte(&self) -> bool {
        self.ctrl || self.alt
    }
}

impl fmt::Display for Atalho {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.ctrl {
            f.write_str("Ctrl+")?;
        }
        if self.alt {
            f.write_str("Alt+")?;
        }
        if self.shift {
            f.write_str("Shift+")?;
        }
        let nome = match self.tecla {
            Key::ArrowLeft => "←",
            Key::ArrowRight => "→",
            Key::ArrowUp => "↑",
            Key::ArrowDown => "↓",
            outra => outra.name(),
        };
        f.write_str(nome)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn le_e_escreve_o_texto_canonico() {
        let a = Atalho::ler("Ctrl+K").unwrap();
        assert_eq!(a, Atalho::ctrl(Key::K));
        assert_eq!(a.to_string(), "Ctrl+K");
        let b = Atalho::ler("alt+ArrowLeft").unwrap();
        assert_eq!(b, Atalho::alt(Key::ArrowLeft));
        assert_eq!(b.to_string(), "Alt+←");
        assert_eq!(
            Atalho::ler("Ctrl+Shift+F1").unwrap(),
            Atalho::ctrl(Key::F1).com_shift()
        );
        assert!(Atalho::ler("Hiper+K").is_err());
        assert!(Atalho::ler("Ctrl+Nada").is_err());
    }

    #[test]
    fn modificadores_sao_comparados_com_exatidao() {
        let voltar = Atalho::alt(Key::ArrowLeft);
        let alt = Modifiers::ALT;
        assert!(voltar.bate(Key::ArrowLeft, alt));
        assert!(!voltar.bate(Key::ArrowLeft, Modifiers::NONE));
        assert!(!voltar.bate(Key::ArrowLeft, alt | Modifiers::SHIFT));
        assert!(Atalho::ctrl(Key::K).bate(Key::K, Modifiers::COMMAND));
    }
}
