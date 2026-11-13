//! Relógio injetado. Limites, cache e prazos só conhecem instantes vindos daqui; nos testes o
//! [`RelogioFalso`] faz "esperar" avançar o tempo na hora.

use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Instante de parede em milissegundos desde 1970-01-01 UTC. Persistível: a idade do cache
/// sobrevive ao reinício do app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Instante(i64);

impl Instante {
    pub const fn de_milissegundos(ms: i64) -> Self {
        Self(ms)
    }

    pub const fn em_milissegundos(self) -> i64 {
        self.0
    }

    pub fn mais(self, duracao: Duration) -> Self {
        Self(self.0.saturating_add(milissegundos(duracao)))
    }

    /// Tempo decorrido desde `antes`; zero se `antes` está no futuro (relógio voltou).
    pub fn desde(self, antes: Instante) -> Duration {
        Duration::from_millis(u64::try_from(self.0.saturating_sub(antes.0)).unwrap_or(0))
    }
}

pub(crate) fn milissegundos(duracao: Duration) -> i64 {
    i64::try_from(duracao.as_millis()).unwrap_or(i64::MAX)
}

/// Fonte de tempo do portão.
pub trait Relogio: Send + Sync + fmt::Debug {
    fn agora(&self) -> Instante;

    /// Espera até `prazo`. Devolve `false` se `cancelar` foi acionado antes.
    fn dormir_ate(&self, prazo: Instante, cancelar: &AtomicBool) -> bool;
}

/// Relógio do sistema; a espera é fatiada para atender ao cancelamento.
#[derive(Debug, Clone, Copy, Default)]
pub struct RelogioDoSistema;

impl Relogio for RelogioDoSistema {
    fn agora(&self) -> Instante {
        let ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX));
        Instante(ms)
    }

    fn dormir_ate(&self, prazo: Instante, cancelar: &AtomicBool) -> bool {
        const FATIA: Duration = Duration::from_millis(25);
        loop {
            if cancelar.load(Ordering::Acquire) {
                return false;
            }
            let falta = prazo.desde(self.agora());
            if falta.is_zero() {
                return true;
            }
            std::thread::sleep(falta.min(FATIA));
        }
    }
}

/// Relógio de teste: parado até alguém avançá-lo; esperar só avança o ponteiro.
#[derive(Debug)]
pub struct RelogioFalso {
    agora: AtomicI64,
}

impl RelogioFalso {
    pub fn novo(inicio: Instante) -> Self {
        Self {
            agora: AtomicI64::new(inicio.0),
        }
    }

    pub fn avancar(&self, quanto: Duration) {
        self.agora
            .fetch_add(milissegundos(quanto), Ordering::SeqCst);
    }

    pub fn definir(&self, instante: Instante) {
        self.agora.store(instante.0, Ordering::SeqCst);
    }
}

impl Relogio for RelogioFalso {
    fn agora(&self) -> Instante {
        Instante(self.agora.load(Ordering::SeqCst))
    }

    fn dormir_ate(&self, prazo: Instante, cancelar: &AtomicBool) -> bool {
        if cancelar.load(Ordering::Acquire) {
            return false;
        }
        self.agora.fetch_max(prazo.0, Ordering::SeqCst);
        true
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn o_relogio_falso_so_anda_quando_mandam() {
        let relogio = RelogioFalso::novo(Instante::de_milissegundos(1_000));
        assert_eq!(relogio.agora(), Instante::de_milissegundos(1_000));
        relogio.avancar(Duration::from_secs(2));
        assert_eq!(relogio.agora().em_milissegundos(), 3_000);
        let parar = AtomicBool::new(false);
        assert!(relogio.dormir_ate(Instante::de_milissegundos(10_000), &parar));
        assert_eq!(relogio.agora().em_milissegundos(), 10_000);
        parar.store(true, Ordering::SeqCst);
        assert!(!relogio.dormir_ate(Instante::de_milissegundos(20_000), &parar));
    }

    #[test]
    fn desde_nunca_fica_negativo() {
        let a = Instante::de_milissegundos(5);
        let b = Instante::de_milissegundos(10);
        assert_eq!(b.desde(a), Duration::from_millis(5));
        assert_eq!(a.desde(b), Duration::ZERO);
    }
}
