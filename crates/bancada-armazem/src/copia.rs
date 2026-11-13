//! Cópia do banco pela API de backup do SQLite e verificação de integridade.

use crate::erro::{Erro, Resultado};
use rusqlite::backup::Backup;
use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Páginas por passo: cada passo segura o lock de leitura da origem só por esse tempo.
const PAGINAS_POR_PASSO: i32 = 128;
const PAUSA_ENTRE_PASSOS: Duration = Duration::from_millis(2);

fn caminho_parcial(destino: &Path) -> PathBuf {
    let mut nome = destino.as_os_str().to_owned();
    nome.push(format!(".parcial-{}", std::process::id()));
    PathBuf::from(nome)
}

fn copiar_para_parcial(origem: &Connection, parcial: &Path) -> Resultado<()> {
    let mut copia = Connection::open(parcial)?;
    {
        let backup = Backup::new(origem, &mut copia)?;
        backup.run_to_completion(PAGINAS_POR_PASSO, PAUSA_ENTRE_PASSOS, None)?;
    }
    // A cópia sai como um arquivo só: sem `-wal`/`-shm` ao lado.
    copia.query_row("PRAGMA journal_mode = DELETE", [], |_| Ok(()))?;
    copia.close().map_err(|(_, e)| Erro::Sqlite(e))?;
    Ok(())
}

/// Copia o banco de `origem` (consistente, inclusive o que ainda está no WAL, mesmo com
/// outros escrevendo) para `destino`.
///
/// A cópia é feita num arquivo `<destino>.parcial-<pid>` e renomeada no fim: ninguém vê uma
/// cópia pela metade. Nunca sobrescreve: se `destino` existe, devolve
/// [`Erro::DestinoJaExiste`]. A pasta de `destino` precisa existir.
///
/// # Errors
/// [`Erro::DestinoJaExiste`], [`Erro::Sqlite`] ou [`Erro::Io`]. Em qualquer falha o arquivo
/// parcial é removido.
pub fn copiar_banco(origem: &Connection, destino: &Path) -> Resultado<()> {
    if destino.exists() {
        return Err(Erro::DestinoJaExiste(destino.to_path_buf()));
    }
    let parcial = caminho_parcial(destino);
    let resultado = copiar_para_parcial(origem, &parcial)
        .and_then(|()| std::fs::rename(&parcial, destino).map_err(Erro::from));
    if resultado.is_err() {
        for sufixo in ["", "-wal", "-shm", "-journal"] {
            let mut nome = parcial.as_os_str().to_owned();
            nome.push(sufixo);
            let _ = std::fs::remove_file(PathBuf::from(nome));
        }
    }
    resultado
}

/// Roda `PRAGMA integrity_check`; qualquer resposta além de `ok` vira erro.
///
/// # Errors
/// [`Erro::IntegridadeComFalhas`] com as linhas devolvidas, ou falha do SQLite.
pub fn verificar_integridade(conexao: &Connection) -> Resultado<()> {
    let mut consulta = conexao.prepare("PRAGMA integrity_check")?;
    let linhas = consulta
        .query_map([], |l| l.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    if linhas.len() == 1 && linhas[0] == "ok" {
        Ok(())
    } else {
        Err(Erro::IntegridadeComFalhas(linhas))
    }
}

/// Abre `caminho` só para leitura e roda [`verificar_integridade`]. Serve para conferir uma
/// cópia recém-feita. Um arquivo que nem abre como banco também é falha de integridade.
///
/// # Errors
/// [`Erro::IntegridadeComFalhas`] ou falha do SQLite ao abrir.
pub fn verificar_integridade_de_arquivo(caminho: &Path) -> Resultado<()> {
    let conexao = Connection::open_with_flags(
        caminho,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    match verificar_integridade(&conexao) {
        // "file is not a database" e "malformed" aparecem como erro do SQLite, não como linhas.
        Err(Erro::Sqlite(e)) => Err(Erro::IntegridadeComFalhas(vec![e.to_string()])),
        outro => outro,
    }
}
