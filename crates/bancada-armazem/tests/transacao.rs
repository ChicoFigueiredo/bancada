//! Transação IMMEDIATE curta: sempre termina em commit ou rollback.

mod common;

use bancada_armazem::{Armazem, Erro, Pragmas};
use common::PastaTemporaria;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Duration;

fn com_tabela(pasta: &PastaTemporaria) -> Armazem {
    let a = Armazem::abrir(pasta.banco(), &Pragmas::default()).expect("abrir");
    a.conexao()
        .execute_batch("CREATE TABLE t (x INTEGER NOT NULL);")
        .expect("esquema");
    a
}

fn contar(a: &Armazem) -> i64 {
    a.conexao()
        .query_row("SELECT COUNT(*) FROM t", [], |l| l.get(0))
        .expect("contar")
}

#[test]
fn sucesso_faz_commit_e_devolve_o_valor() {
    let pasta = PastaTemporaria::nova("commit");
    let mut a = com_tabela(&pasta);
    let id = a
        .transacao_imediata(|tx| -> Result<i64, Erro> {
            tx.execute("INSERT INTO t VALUES (1)", [])?;
            Ok(tx.last_insert_rowid())
        })
        .expect("transação");
    assert_eq!(id, 1);
    assert_eq!(contar(&a), 1);
    assert!(
        a.conexao().is_autocommit(),
        "nenhuma transação ficou aberta"
    );
}

#[test]
fn erro_do_chamador_desfaz_tudo_e_nao_deixa_transacao_aberta() {
    let pasta = PastaTemporaria::nova("rollback");
    let mut a = com_tabela(&pasta);
    let resultado = a.transacao_imediata(|tx| -> Result<(), Erro> {
        tx.execute("INSERT INTO t VALUES (1)", [])?;
        tx.execute("INSERT INTO t VALUES (2)", [])?;
        Err(Erro::NomeInvalido("falha do chamador".into()))
    });
    assert!(matches!(resultado, Err(Erro::NomeInvalido(_))));
    assert_eq!(contar(&a), 0, "nenhuma linha ficou");
    assert!(
        a.conexao().is_autocommit(),
        "nenhuma transação ficou aberta"
    );

    // O lock de escrita foi solto: outro processo grava de imediato, sem esperar.
    let pragmas = Pragmas {
        espera_ocupado: Duration::ZERO,
        ..Pragmas::default()
    };
    let mut outro = Armazem::abrir(pasta.banco(), &pragmas).expect("abrir");
    outro
        .transacao_imediata(|tx| -> Result<(), Erro> {
            tx.execute("INSERT INTO t VALUES (3)", [])?;
            Ok(())
        })
        .expect("o lock estava solto");
}

#[test]
fn falha_do_sqlite_no_meio_desfaz_o_lote() {
    let pasta = PastaTemporaria::nova("sqlite-falha");
    let mut a = com_tabela(&pasta);
    let resultado = a.transacao_imediata(|tx| -> Result<(), Erro> {
        tx.execute("INSERT INTO t VALUES (1)", [])?;
        tx.execute("INSERT INTO t VALUES (NULL)", [])?; // viola NOT NULL
        Ok(())
    });
    assert!(matches!(resultado, Err(Erro::Sqlite(_))));
    assert_eq!(contar(&a), 0);
    assert!(a.conexao().is_autocommit());
}

#[test]
fn panico_dentro_da_transacao_tambem_desfaz() {
    let pasta = PastaTemporaria::nova("panico");
    let mut a = com_tabela(&pasta);
    let r = catch_unwind(AssertUnwindSafe(|| {
        let _ = a.transacao_imediata(|tx| -> Result<(), Erro> {
            tx.execute("INSERT INTO t VALUES (1)", [])?;
            panic!("explodiu no meio");
        });
    }));
    assert!(r.is_err());
    assert_eq!(contar(&a), 0);
    assert!(a.conexao().is_autocommit());
}

#[test]
fn escritores_concorrentes_se_serializam_sem_database_is_locked() {
    // A lição do inventário: vários processos (aqui, conexões) no mesmo arquivo, lotes curtos
    // IMMEDIATE e `busy_timeout` longo. Nenhum incremento se perde, nenhum erro de lock.
    let pasta = PastaTemporaria::nova("concorrencia");
    let caminho = pasta.banco();
    {
        let a = Armazem::abrir(&caminho, &Pragmas::default()).expect("abrir");
        a.conexao()
            .execute_batch(
                "CREATE TABLE contador (n INTEGER NOT NULL); INSERT INTO contador VALUES (0);",
            )
            .expect("esquema");
    }
    let pragmas = Pragmas {
        espera_ocupado: Duration::from_secs(30),
        ..Pragmas::default()
    };
    let fios: Vec<_> = (0..4)
        .map(|_| {
            let (caminho, pragmas) = (caminho.clone(), pragmas.clone());
            std::thread::spawn(move || {
                let mut a = Armazem::abrir(&caminho, &pragmas).expect("abrir");
                for _ in 0..25 {
                    a.transacao_imediata(|tx| -> Result<(), Erro> {
                        // Lê e grava dentro do mesmo lote: sem upgrade de leitura para escrita.
                        let n: i64 = tx.query_row("SELECT n FROM contador", [], |l| l.get(0))?;
                        tx.execute("UPDATE contador SET n = ?1", [n + 1])?;
                        Ok(())
                    })
                    .expect("lote");
                }
            })
        })
        .collect();
    for f in fios {
        f.join().expect("fio");
    }
    let a = Armazem::abrir(&caminho, &Pragmas::default()).expect("abrir");
    let n: i64 = a
        .conexao()
        .query_row("SELECT n FROM contador", [], |l| l.get(0))
        .expect("ler");
    assert_eq!(n, 100);
}
