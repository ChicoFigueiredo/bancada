//! Tetos do VM, resultado do roteiro e erros de execução.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use super::verbos::ErroDeVerbo;

/// Tetos do VM. Estourar qualquer um termina o roteiro com um erro claro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limites {
    /// Teto de "instruções". O Luau avisa o host a cada chamada de função e a cada volta de
    /// laço; cada aviso conta como uma instrução. É uma medida aproximada, boa para barrar
    /// laços infinitos.
    pub instrucoes: u64,
    /// Memória que o roteiro pode alocar, em bytes.
    pub memoria: usize,
    /// Tempo total de relógio. Não conta o tempo em que o roteiro esperou o usuário (`pausa`).
    pub tempo_total: Duration,
    /// Número máximo de chamadas a verbos (todos, inclusive `registrar` e `util.*`).
    pub chamadas: u32,
    /// Tamanho máximo do fonte, em bytes.
    pub tamanho_do_fonte: usize,
}

impl Default for Limites {
    fn default() -> Self {
        Self {
            instrucoes: 5_000_000,
            memoria: 32 * 1024 * 1024,
            tempo_total: Duration::from_secs(10 * 60),
            chamadas: 2_000,
            tamanho_do_fonte: 64 * 1024,
        }
    }
}

/// O que o roteiro produziu ao terminar sem erro.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Saida {
    /// Nomes dos arquivos devolvidos por `navegador.baixar`, na ordem.
    pub arquivos: Vec<String>,
    /// Mensagens passadas a `navegador.registrar`, na ordem (o que o script escreveu).
    pub registros: Vec<String>,
}

/// Por que o roteiro não terminou bem.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ErroDeRoteiro {
    /// O fonte passa do tamanho máximo.
    #[error("o roteiro tem {tamanho} bytes e o máximo é {maximo}")]
    FonteGrande {
        /// Tamanho recebido.
        tamanho: usize,
        /// Tamanho permitido.
        maximo: usize,
    },
    /// O fonte não compila.
    #[error("erro de sintaxe{}: {mensagem}", linha_em(*.linha))]
    Sintaxe {
        /// Mensagem do compilador do Luau.
        mensagem: String,
        /// Linha, quando o compilador informa.
        linha: Option<u32>,
    },
    /// O script (ou um tipo de argumento errado) gerou um erro em tempo de execução.
    #[error("erro no roteiro{}: {mensagem}", linha_em(*.linha))]
    Execucao {
        /// Mensagem do erro.
        mensagem: String,
        /// Linha do roteiro onde o erro aconteceu, quando dá para saber.
        linha: Option<u32>,
    },
    /// Um verbo falhou e o script não capturou o erro.
    #[error("{erro}{}", linha_em(*.linha))]
    Verbo {
        /// O erro do verbo.
        erro: ErroDeVerbo,
        /// Linha da chamada, quando dá para saber.
        linha: Option<u32>,
    },
    /// Passou do teto de instruções.
    #[error("o roteiro passou do limite de {0} instruções (laço infinito?)")]
    LimiteDeInstrucoes(u64),
    /// Passou do tempo total.
    #[error("o roteiro passou do tempo total de {0:?}")]
    LimiteDeTempo(Duration),
    /// Passou do número de chamadas a verbos.
    #[error("o roteiro passou do limite de {0} chamadas a verbos")]
    LimiteDeChamadas(u32),
    /// Passou do teto de memória.
    #[error("o roteiro passou do limite de memória de {0} bytes")]
    LimiteDeMemoria(usize),
    /// Falha do próprio host (não do roteiro).
    #[error("falha interna do host de roteiros: {0}")]
    Interno(String),
}

fn linha_em(linha: Option<u32>) -> String {
    linha.map(|l| format!(" (linha {l})")).unwrap_or_default()
}

/// Estado compartilhado com o interrupt do Luau (que precisa ser `'static`).
#[derive(Debug)]
pub(crate) struct Controle {
    limites: Limites,
    inicio: Instant,
    instrucoes: AtomicU64,
    /// Nanossegundos que o roteiro passou esperando o usuário.
    pausado_ns: AtomicU64,
    /// O erro que encerrou o roteiro, quando foi o host (limite ou verbo fatal) que decidiu.
    fatal: Mutex<Option<ErroDeRoteiro>>,
}

impl Controle {
    pub(crate) fn novo(limites: Limites) -> Self {
        Self {
            limites,
            inicio: Instant::now(),
            instrucoes: AtomicU64::new(0),
            pausado_ns: AtomicU64::new(0),
            fatal: Mutex::new(None),
        }
    }

    pub(crate) fn limites(&self) -> &Limites {
        &self.limites
    }

    /// Registra o erro que encerra o roteiro (o primeiro vence).
    pub(crate) fn marcar_fatal(&self, erro: ErroDeRoteiro) {
        if let Ok(mut f) = self.fatal.lock() {
            f.get_or_insert(erro);
        }
    }

    pub(crate) fn esta_encerrado(&self) -> bool {
        self.fatal.lock().map(|f| f.is_some()).unwrap_or(true)
    }

    pub(crate) fn tomar_fatal(&self) -> Option<ErroDeRoteiro> {
        self.fatal.lock().ok().and_then(|mut f| f.take())
    }

    pub(crate) fn somar_pausa(&self, d: Duration) {
        let ns = u64::try_from(d.as_nanos()).unwrap_or(u64::MAX);
        self.pausado_ns.fetch_add(ns, Ordering::Relaxed);
    }

    /// Chamado a cada aviso do Luau. `Err` com o motivo encerra o roteiro.
    pub(crate) fn tique(&self) -> Result<(), &'static str> {
        if self.esta_encerrado() {
            return Err("roteiro encerrado");
        }
        let n = self.instrucoes.fetch_add(1, Ordering::Relaxed) + 1;
        if n > self.limites.instrucoes {
            self.marcar_fatal(ErroDeRoteiro::LimiteDeInstrucoes(self.limites.instrucoes));
            return Err("limite de instruções");
        }
        // O relógio custa mais que um contador: olha de vez em quando.
        if n.is_multiple_of(256) {
            self.conferir_tempo()?;
        }
        Ok(())
    }

    /// Confere o tempo total (sem contar as pausas).
    pub(crate) fn conferir_tempo(&self) -> Result<(), &'static str> {
        let pausado = Duration::from_nanos(self.pausado_ns.load(Ordering::Relaxed));
        let gasto = self.inicio.elapsed().saturating_sub(pausado);
        if gasto > self.limites.tempo_total {
            self.marcar_fatal(ErroDeRoteiro::LimiteDeTempo(self.limites.tempo_total));
            return Err("limite de tempo");
        }
        Ok(())
    }
}
