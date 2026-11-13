//! Cópia pela API de backup do SQLite e verificação de integridade.

mod common;

use bancada_armazem::{Armazem, Erro, Pragmas, verificar_integridade_de_arquivo};
use common::PastaTemporaria;
use rusqlite::Connection;

fn povoado(pasta: &PastaTemporaria) -> Armazem {
    let a = Armazem::abrir(pasta.banco(), &Pragmas::default()).expect("abrir");
    a.conexao()
        .execute_batch(
            "CREATE TABLE t (id INTEGER PRIMARY KEY, texto TEXT NOT NULL);
             CREATE INDEX t_texto ON t (texto);
             PRAGMA user_version = 7;",
        )
        .expect("esquema");
    for i in 0..500 {
        a.conexao()
            .execute("INSERT INTO t (texto) VALUES (?1)", [format!("linha {i}")])
            .expect("inserir");
    }
    a
}

fn conteudo(c: &Connection) -> Vec<(i64, String)> {
    c.prepare("SELECT id, texto FROM t ORDER BY id")
        .expect("preparar")
        .query_map([], |l| Ok((l.get(0)?, l.get(1)?)))
        .expect("consultar")
        .collect::<Result<_, _>>()
        .expect("linhas")
}

#[test]
fn backup_reproduz_o_conteudo_e_a_versao_inclusive_o_que_esta_no_wal() {
    let pasta = PastaTemporaria::nova("backup");
    let a = povoado(&pasta);
    // Com WAL e o app aberto, parte dos dados ainda está só no -wal: copiar o arquivo cru
    // perderia isso; a API de backup não.
    let destino = pasta.pasta().join("copia.sqlite");
    a.copiar_para(&destino).expect("copiar");

    let copia = Connection::open(&destino).expect("abrir cópia");
    assert_eq!(conteudo(&copia), conteudo(a.conexao()));
    let versao: u32 = copia
        .query_row("PRAGMA user_version", [], |l| l.get(0))
        .expect("versão");
    assert_eq!(versao, 7);
    verificar_integridade_de_arquivo(&destino).expect("a cópia é íntegra");
}

#[test]
fn backup_e_um_arquivo_so_e_nao_deixa_temporario() {
    let pasta = PastaTemporaria::nova("backup-limpo");
    let a = povoado(&pasta);
    let destino = pasta.pasta().join("copia.sqlite");
    a.copiar_para(&destino).expect("copiar");

    let mut nomes: Vec<String> = std::fs::read_dir(pasta.pasta())
        .expect("listar")
        .map(|e| {
            e.expect("entrada")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|n| n.starts_with("copia"))
        .collect();
    nomes.sort();
    assert_eq!(nomes, ["copia.sqlite"], "sem -wal, -shm nem .parcial");
}

#[test]
fn backup_nunca_sobrescreve_um_destino_existente() {
    let pasta = PastaTemporaria::nova("backup-existe");
    let a = povoado(&pasta);
    let destino = pasta.pasta().join("copia.sqlite");
    std::fs::write(&destino, b"importante").expect("escrever");
    let erro = a.copiar_para(&destino).expect_err("deve recusar");
    assert!(matches!(erro, Erro::DestinoJaExiste(_)));
    assert_eq!(std::fs::read(&destino).expect("ler"), b"importante");
}

#[test]
fn backup_falho_nao_deixa_arquivo_parcial() {
    let pasta = PastaTemporaria::nova("backup-falha");
    let a = povoado(&pasta);
    // Pasta de destino inexistente: a cópia falha e nada fica pelo caminho.
    let destino = pasta.pasta().join("nao_existe").join("copia.sqlite");
    assert!(a.copiar_para(&destino).is_err());
    assert!(!destino.exists());
}

#[test]
fn backup_de_banco_em_memoria_funciona() {
    let pasta = PastaTemporaria::nova("backup-memoria");
    let a = Armazem::em_memoria(&Pragmas::default()).expect("abrir");
    a.conexao()
        .execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY, texto TEXT NOT NULL); INSERT INTO t (texto) VALUES ('a');")
        .expect("esquema");
    let destino = pasta.pasta().join("memoria.sqlite");
    a.copiar_para(&destino).expect("copiar");
    let copia = Connection::open(&destino).expect("abrir cópia");
    assert_eq!(conteudo(&copia), vec![(1, "a".to_owned())]);
}

#[test]
fn integridade_de_banco_sadio_passa() {
    let pasta = PastaTemporaria::nova("integridade-ok");
    let a = povoado(&pasta);
    a.verificar_integridade().expect("íntegro");
}

#[test]
fn integridade_acusa_arquivo_corrompido() {
    let pasta = PastaTemporaria::nova("integridade-ruim");
    {
        let a = povoado(&pasta);
        a.copiar_para(pasta.pasta().join("copia.sqlite"))
            .expect("copiar");
    }
    let copia = pasta.pasta().join("copia.sqlite");
    // Estraga páginas do meio do arquivo (mantém o cabeçalho, que ainda abre).
    let mut bytes = std::fs::read(&copia).expect("ler");
    assert!(bytes.len() > 3 * 4096);
    for b in &mut bytes[4096..3 * 4096] {
        *b = 0xA5;
    }
    std::fs::write(&copia, bytes).expect("escrever");

    let erro = verificar_integridade_de_arquivo(&copia);
    assert!(erro.is_err(), "corrupção deveria ser acusada: {erro:?}");
}
