//! PRAGMAs configuráveis de uma conexão.

use crate::erro::Resultado;
use rusqlite::Connection;
use rusqlite::types::Value;
use std::time::Duration;

/// Modo do diário (`journal_mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModoDiario {
    /// WAL: leitores não bloqueiam o escritor e a queda não corrompe o arquivo.
    Wal,
    /// Diário de reversão clássico (`DELETE`).
    Delete,
}

impl ModoDiario {
    fn como_sql(self) -> &'static str {
        match self {
            Self::Wal => "WAL",
            Self::Delete => "DELETE",
        }
    }
}

/// Nível de `synchronous`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sincronizacao {
    /// `OFF` (0): sem fsync; só para dados descartáveis.
    Desligada,
    /// `NORMAL` (1).
    Normal,
    /// `FULL` (2): o commit só volta após o fsync.
    Completa,
    /// `EXTRA` (3).
    Extra,
}

impl Sincronizacao {
    fn como_sql(self) -> &'static str {
        match self {
            Self::Desligada => "OFF",
            Self::Normal => "NORMAL",
            Self::Completa => "FULL",
            Self::Extra => "EXTRA",
        }
    }
}

/// Configuração dos PRAGMAs. O padrão é o do Chess Studio: WAL, `synchronous=FULL`,
/// chaves estrangeiras ligadas e espera de 5 s por um lock.
///
/// Para vários processos no mesmo arquivo (os scripts do inventário), aumente
/// [`espera_ocupado`](Self::espera_ocupado) (a lição do inventário usa 30 s).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pragmas {
    /// `journal_mode`.
    pub modo_diario: ModoDiario,
    /// `busy_timeout`: quanto esperar por um lock antes de devolver `SQLITE_BUSY`.
    pub espera_ocupado: Duration,
    /// `foreign_keys`.
    pub chaves_estrangeiras: bool,
    /// `synchronous`.
    pub sincronizacao: Sincronizacao,
    /// `recursive_triggers`. Ligado por padrão: sem isso, `INSERT OR REPLACE` apaga a linha
    /// antiga **sem** disparar o trigger de `DELETE`, e furaria as tabelas só de acréscimo.
    pub gatilhos_recursivos: bool,
}

impl Default for Pragmas {
    fn default() -> Self {
        Self {
            modo_diario: ModoDiario::Wal,
            espera_ocupado: Duration::from_secs(5),
            chaves_estrangeiras: true,
            sincronizacao: Sincronizacao::Completa,
            gatilhos_recursivos: true,
        }
    }
}

impl Pragmas {
    /// Aplica a configuração. Em banco em memória o SQLite mantém o modo `memory`; isso é
    /// esperado e não é erro.
    pub(crate) fn aplicar(&self, conexao: &Connection) -> Resultado<()> {
        // A espera vem primeiro: trocar o modo do diário também pode esbarrar num lock.
        conexao.busy_timeout(self.espera_ocupado)?;
        conexao.pragma_update(None, "foreign_keys", self.chaves_estrangeiras)?;
        // `journal_mode` devolve uma linha; ela é ignorada de propósito.
        conexao.query_row(
            &format!("PRAGMA journal_mode = {}", self.modo_diario.como_sql()),
            [],
            |_| Ok(()),
        )?;
        conexao.pragma_update(None, "synchronous", self.sincronizacao.como_sql())?;
        conexao.pragma_update(None, "recursive_triggers", self.gatilhos_recursivos)?;
        Ok(())
    }

    /// Só o que vale para conexão de leitura (que não troca o modo do diário).
    pub(crate) fn aplicar_na_leitura(&self, conexao: &Connection) -> Resultado<()> {
        conexao.busy_timeout(self.espera_ocupado)?;
        conexao.pragma_update(None, "foreign_keys", self.chaves_estrangeiras)?;
        Ok(())
    }
}

/// Valores efetivos dos PRAGMAs numa conexão (diagnóstico e testes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PragmasLidos {
    /// `journal_mode` em minúsculas (`wal`, `delete`, `memory`...).
    pub modo_diario: String,
    /// `foreign_keys`.
    pub chaves_estrangeiras: bool,
    /// 0 OFF, 1 NORMAL, 2 FULL, 3 EXTRA.
    pub sincronizacao: i64,
    /// `busy_timeout` em milissegundos.
    pub espera_ocupado_ms: i64,
    /// `recursive_triggers`.
    pub gatilhos_recursivos: bool,
}

/// Lê os PRAGMAs efetivos de `conexao`.
///
/// # Errors
/// Falha do SQLite ao ler.
pub fn ler_pragmas(conexao: &Connection) -> Resultado<PragmasLidos> {
    let consulta = |nome: &str| -> Resultado<Value> {
        Ok(conexao.query_row(&format!("PRAGMA {nome}"), [], |l| l.get(0))?)
    };
    let inteiro = |v: Value| match v {
        Value::Integer(i) => i,
        _ => -1,
    };
    let modo_diario = match consulta("journal_mode")? {
        Value::Text(t) => t,
        _ => String::new(),
    };
    Ok(PragmasLidos {
        modo_diario,
        chaves_estrangeiras: inteiro(consulta("foreign_keys")?) == 1,
        sincronizacao: inteiro(consulta("synchronous")?),
        espera_ocupado_ms: inteiro(consulta("busy_timeout")?),
        gatilhos_recursivos: inteiro(consulta("recursive_triggers")?) == 1,
    })
}
