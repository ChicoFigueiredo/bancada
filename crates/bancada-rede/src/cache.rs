//! O cache de respostas do portão, num SQLite próprio (pela escada do `bancada-armazem`). É
//! descartável: apagar o arquivo não perde nada além de pedidos já feitos. Guarda só as
//! respostas `200` de pedidos que pediram ([`crate::Pedido::guardar_por`]), com a `ETag` para
//! revalidar e o instante da consulta para mostrar a idade offline.

use crate::erro::ErroDeRede;
use crate::relogio::Instante;
use bancada_armazem::{Armazem, Escada, Migracao, Pragmas, Sincronizacao};
use rusqlite::{OptionalExtension, params};
use std::path::Path;

const MIGRACOES: &[Migracao] = &[Migracao::sql(
    "cache de respostas com ETag",
    "CREATE TABLE resposta (
        chave          TEXT PRIMARY KEY,
        origem         TEXT NOT NULL,
        status         INTEGER NOT NULL,
        tipo           TEXT,
        corpo          BLOB NOT NULL,
        etag           TEXT,
        consultada_ms  INTEGER NOT NULL,
        vence_ms       INTEGER NOT NULL
    ) WITHOUT ROWID;",
)];

/// Uma resposta guardada.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Guardada {
    pub origem: String,
    pub status: u16,
    pub tipo: Option<String>,
    pub corpo: Vec<u8>,
    pub etag: Option<String>,
    pub consultada_em: Instante,
    pub vence_em: Instante,
}

impl std::fmt::Debug for Guardada {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Guardada")
            .field("origem", &self.origem)
            .field("status", &self.status)
            .field("corpo", &self.corpo.len())
            .finish_non_exhaustive()
    }
}

impl Guardada {
    pub fn esta_fresca(&self, agora: Instante) -> bool {
        agora < self.vence_em
    }
}

#[derive(Debug)]
pub(crate) struct Cache {
    armazem: Armazem,
}

fn erro(e: impl std::fmt::Display) -> ErroDeRede {
    ErroDeRede::armazenamento(e)
}

fn pragmas() -> Pragmas {
    // Descartável: sem fsync a cada gravação.
    Pragmas {
        sincronizacao: Sincronizacao::Normal,
        ..Pragmas::default()
    }
}

impl Cache {
    pub fn abrir(caminho: &Path) -> Result<Self, ErroDeRede> {
        Self::migrado(Armazem::abrir(caminho, &pragmas()).map_err(erro)?)
    }

    pub fn em_memoria() -> Result<Self, ErroDeRede> {
        Self::migrado(Armazem::em_memoria(&pragmas()).map_err(erro)?)
    }

    fn migrado(mut armazem: Armazem) -> Result<Self, ErroDeRede> {
        armazem.migrar(&Escada::nova(MIGRACOES)).map_err(erro)?;
        Ok(Self { armazem })
    }

    pub fn ler(&self, chave: &str) -> Result<Option<Guardada>, ErroDeRede> {
        self.armazem
            .conexao()
            .query_row(
                "SELECT origem, status, tipo, corpo, etag, consultada_ms, vence_ms
                 FROM resposta WHERE chave = ?1",
                [chave],
                |l| {
                    Ok(Guardada {
                        origem: l.get(0)?,
                        status: l.get(1)?,
                        tipo: l.get(2)?,
                        corpo: l.get(3)?,
                        etag: l.get(4)?,
                        consultada_em: Instante::de_milissegundos(l.get(5)?),
                        vence_em: Instante::de_milissegundos(l.get(6)?),
                    })
                },
            )
            .optional()
            .map_err(erro)
    }

    pub fn gravar(&self, chave: &str, g: &Guardada) -> Result<(), ErroDeRede> {
        self.armazem
            .conexao()
            .execute(
                "INSERT INTO resposta (chave, origem, status, tipo, corpo, etag, consultada_ms, vence_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT (chave) DO UPDATE SET
                    origem = excluded.origem, status = excluded.status, tipo = excluded.tipo,
                    corpo = excluded.corpo, etag = excluded.etag,
                    consultada_ms = excluded.consultada_ms, vence_ms = excluded.vence_ms",
                params![
                    chave,
                    g.origem,
                    g.status,
                    g.tipo,
                    g.corpo,
                    g.etag,
                    g.consultada_em.em_milissegundos(),
                    g.vence_em.em_milissegundos()
                ],
            )
            .map_err(erro)?;
        Ok(())
    }

    /// `304`: o conteúdo segue valendo; renova o instante da consulta e a validade.
    pub fn renovar(
        &self,
        chave: &str,
        consultada_em: Instante,
        vence_em: Instante,
        etag: Option<&str>,
    ) -> Result<(), ErroDeRede> {
        self.armazem
            .conexao()
            .execute(
                "UPDATE resposta SET consultada_ms = ?2, vence_ms = ?3, etag = COALESCE(?4, etag)
                 WHERE chave = ?1",
                params![
                    chave,
                    consultada_em.em_milissegundos(),
                    vence_em.em_milissegundos(),
                    etag
                ],
            )
            .map_err(erro)?;
        Ok(())
    }

    pub fn limpar(&self) -> Result<usize, ErroDeRede> {
        self.armazem
            .conexao()
            .execute("DELETE FROM resposta", [])
            .map_err(erro)
    }
}
