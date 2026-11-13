//! Apoio de teste (feature `testes`, só em `[dev-dependencies]`): um keyring do sistema falso,
//! em memória, para a suíte nunca depender de haver keyring na máquina, nunca gravar no keyring
//! real do usuário e nunca abrir diálogo do SO.

use crate::chaveiro::ChaveiroDoSistema;
use crate::erro::FalhaDoChaveiro;
use crate::segredo::Segredo;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

/// Keyring falso. Pode fingir que não existe ([`ChaveiroEmMemoria::indisponivel`]) ou que
/// falha ([`ChaveiroEmMemoria::falhar`]).
#[derive(Debug)]
pub struct ChaveiroEmMemoria {
    entradas: Mutex<BTreeMap<(String, String), Segredo>>,
    disponivel: bool,
    falhando: AtomicBool,
}

impl Default for ChaveiroEmMemoria {
    fn default() -> Self {
        Self {
            entradas: Mutex::new(BTreeMap::new()),
            disponivel: true,
            falhando: AtomicBool::new(false),
        }
    }
}

impl ChaveiroEmMemoria {
    /// Um keyring que responde.
    pub fn novo() -> Self {
        Self::default()
    }

    /// Sistema sem keyring (Linux sem Secret Service, sessão sem D-Bus).
    pub fn indisponivel() -> Self {
        Self {
            disponivel: false,
            ..Self::default()
        }
    }

    /// Cada operação passa a falhar (o D-Bus caiu, acesso negado).
    pub fn falhar(&self, falhando: bool) {
        self.falhando.store(falhando, Ordering::SeqCst);
    }

    /// Pares (serviço, rótulo) com valor, em ordem.
    pub fn entradas(&self) -> Vec<(String, String)> {
        self.travar().keys().cloned().collect()
    }

    /// Põe um valor direto no keyring falso (o que outro app teria gravado).
    pub fn semear(&self, servico: &str, rotulo: &str, valor: &str) {
        self.travar().insert(
            (servico.to_owned(), rotulo.to_owned()),
            Segredo::novo(valor),
        );
    }

    fn travar(&self) -> MutexGuard<'_, BTreeMap<(String, String), Segredo>> {
        self.entradas.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn conferir(&self) -> Result<(), FalhaDoChaveiro> {
        if !self.disponivel {
            return Err(FalhaDoChaveiro::Indisponivel);
        }
        if self.falhando.load(Ordering::SeqCst) {
            return Err(FalhaDoChaveiro::Outra);
        }
        Ok(())
    }
}

impl ChaveiroDoSistema for ChaveiroEmMemoria {
    fn esta_disponivel(&self) -> bool {
        self.disponivel
    }

    fn gravar(&self, servico: &str, rotulo: &str, valor: &Segredo) -> Result<(), FalhaDoChaveiro> {
        self.conferir()?;
        self.travar()
            .insert((servico.to_owned(), rotulo.to_owned()), valor.clone());
        Ok(())
    }

    fn ler(&self, servico: &str, rotulo: &str) -> Result<Option<Segredo>, FalhaDoChaveiro> {
        self.conferir()?;
        Ok(self
            .travar()
            .get(&(servico.to_owned(), rotulo.to_owned()))
            .cloned())
    }

    fn apagar(&self, servico: &str, rotulo: &str) -> Result<(), FalhaDoChaveiro> {
        self.conferir()?;
        self.travar()
            .remove(&(servico.to_owned(), rotulo.to_owned()));
        Ok(())
    }
}
