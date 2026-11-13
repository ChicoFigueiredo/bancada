//! O armazém: uma conexão de escrita e, para banco em arquivo, uma de leitura separada.

use crate::copia;
use crate::erro::{Erro, Resultado};
use crate::migracoes::{self, Escada, Migracao, RelatorioMigracao};
use crate::pragmas::{Pragmas, PragmasLidos, ler_pragmas};
use crate::transacao::transacao_imediata;
use rusqlite::{Connection, OpenFlags, Transaction};
use std::path::{Path, PathBuf};

/// SQLite aberto com PRAGMAs configurados, pronto para migrar, gravar em lotes curtos e copiar.
///
/// A conexão de escrita é única; a de leitura (só para banco em arquivo) é separada, para a
/// interface ler sem esperar o escritor (WAL).
#[derive(Debug)]
pub struct Armazem {
    escrita: Connection,
    leitura: Option<Connection>,
    caminho: Option<PathBuf>,
}

impl Armazem {
    /// Abre (criando, se preciso) o banco em `caminho`, aplica os `pragmas` e abre a conexão de
    /// leitura. **Não migra**: chame [`migrar`](Self::migrar) em seguida.
    ///
    /// # Errors
    /// Falha do SQLite ao abrir ou configurar.
    pub fn abrir(caminho: impl AsRef<Path>, pragmas: &Pragmas) -> Resultado<Self> {
        let caminho = caminho.as_ref();
        let escrita = Connection::open(caminho)?;
        pragmas.aplicar(&escrita)?;
        // Aberta depois da escrita, para o arquivo e o modo do diário já existirem.
        let leitura = Connection::open_with_flags(
            caminho,
            OpenFlags::SQLITE_OPEN_READ_ONLY
                | OpenFlags::SQLITE_OPEN_URI
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        pragmas.aplicar_na_leitura(&leitura)?;
        Ok(Self {
            escrita,
            leitura: Some(leitura),
            caminho: Some(caminho.to_path_buf()),
        })
    }

    /// Banco em memória (testes e modo sem persistência); não tem conexão de leitura separada.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn em_memoria(pragmas: &Pragmas) -> Resultado<Self> {
        let escrita = Connection::open_in_memory()?;
        pragmas.aplicar(&escrita)?;
        Ok(Self {
            escrita,
            leitura: None,
            caminho: None,
        })
    }

    /// Caminho do arquivo (`None` em memória). Quem trabalha em outra thread abre o seu próprio
    /// [`Armazem`] com ele.
    #[must_use]
    pub fn caminho(&self) -> Option<&Path> {
        self.caminho.as_deref()
    }

    /// A conexão de escrita. Para gravar em lote, prefira [`transacao_imediata`](Self::transacao_imediata).
    #[must_use]
    pub fn conexao(&self) -> &Connection {
        &self.escrita
    }

    /// A conexão de escrita, mutável (necessária para abrir transações).
    pub fn conexao_mut(&mut self) -> &mut Connection {
        &mut self.escrita
    }

    /// A conexão de leitura separada (somente leitura, sem bloquear o escritor em WAL). Não
    /// mantenha um `SELECT` aberto entre quadros: ele fixa um snapshot.
    ///
    /// # Errors
    /// [`Erro::SemArquivo`] em banco em memória.
    pub fn leitura(&self) -> Resultado<&Connection> {
        self.leitura.as_ref().ok_or(Erro::SemArquivo)
    }

    /// Valores efetivos dos PRAGMAs da conexão de escrita.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn pragmas_lidos(&self) -> Resultado<PragmasLidos> {
        ler_pragmas(&self.escrita)
    }

    /// `PRAGMA user_version` do banco.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn versao(&self) -> Resultado<u32> {
        migracoes::versao_do_banco(&self.escrita)
    }

    /// Leva o banco à última versão da `escada`. Veja o módulo de migrações.
    ///
    /// # Errors
    /// [`Erro::BancoMaisNovo`], falhas da linha de base, [`Erro::MigracaoFalhou`] ou SQLite.
    pub fn migrar(&mut self, escada: &Escada<'_>) -> Resultado<RelatorioMigracao> {
        migracoes::migrar(&mut self.escrita, escada)
    }

    /// Leva o banco até `alvo` (no máximo a última versão da `escada`).
    ///
    /// # Errors
    /// Veja [`migrar`](Self::migrar).
    pub fn migrar_ate(&mut self, escada: &Escada<'_>, alvo: u32) -> Resultado<RelatorioMigracao> {
        migracoes::migrar_ate(&mut self.escrita, escada, alvo)
    }

    /// As migrações que ainda vão rodar: o que mostrar ao usuário antes de migrar.
    ///
    /// # Errors
    /// [`Erro::BancoMaisNovo`], [`Erro::EscadaInconsistente`] ou falha do SQLite.
    pub fn pendentes<'a>(&self, escada: &Escada<'a>) -> Resultado<&'a [Migracao]> {
        migracoes::pendentes(&self.escrita, escada)
    }

    /// Transação IMMEDIATE curta: `COMMIT` se `corpo` devolve `Ok`, `ROLLBACK` se devolve `Err`
    /// ou entra em pânico. Veja [`transacao_imediata`](crate::transacao_imediata).
    ///
    /// # Errors
    /// O erro de `corpo`, ou falha do SQLite ao abrir ou confirmar.
    pub fn transacao_imediata<T, E>(
        &mut self,
        corpo: impl FnOnce(&Transaction<'_>) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<rusqlite::Error>,
    {
        transacao_imediata(&mut self.escrita, corpo)
    }

    /// Copia o banco para `destino` pela API de backup do SQLite. Veja
    /// [`copiar_banco`](crate::copiar_banco).
    ///
    /// # Errors
    /// [`Erro::DestinoJaExiste`], [`Erro::Sqlite`] ou [`Erro::Io`].
    pub fn copiar_para(&self, destino: impl AsRef<Path>) -> Resultado<()> {
        copia::copiar_banco(&self.escrita, destino.as_ref())
    }

    /// `PRAGMA integrity_check` do banco aberto.
    ///
    /// # Errors
    /// [`Erro::IntegridadeComFalhas`] ou falha do SQLite.
    pub fn verificar_integridade(&self) -> Resultado<()> {
        copia::verificar_integridade(&self.escrita)
    }
}
