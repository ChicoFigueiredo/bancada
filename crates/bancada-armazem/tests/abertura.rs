//! Abertura do banco: PRAGMAs configuráveis e conexão de leitura separada.

mod common;

use bancada_armazem::{Armazem, Erro, ModoDiario, Pragmas, Sincronizacao};
use common::PastaTemporaria;
use std::time::Duration;

#[test]
fn pragmas_padrao_sao_os_do_chess() {
    let pasta = PastaTemporaria::nova("pragmas");
    let armazem = Armazem::abrir(pasta.banco(), &Pragmas::default()).expect("abrir");
    let lidos = armazem.pragmas_lidos().expect("ler pragmas");
    assert_eq!(lidos.modo_diario, "wal");
    assert!(lidos.chaves_estrangeiras);
    assert_eq!(lidos.sincronizacao, 2, "FULL");
    assert_eq!(lidos.espera_ocupado_ms, 5_000);
    assert!(lidos.gatilhos_recursivos);
}

#[test]
fn pragmas_configuraveis_sao_aplicados() {
    let pasta = PastaTemporaria::nova("pragmas-cfg");
    let pragmas = Pragmas {
        modo_diario: ModoDiario::Delete,
        espera_ocupado: Duration::from_secs(30),
        chaves_estrangeiras: false,
        sincronizacao: Sincronizacao::Normal,
        gatilhos_recursivos: false,
    };
    let armazem = Armazem::abrir(pasta.banco(), &pragmas).expect("abrir");
    let lidos = armazem.pragmas_lidos().expect("ler pragmas");
    assert_eq!(lidos.modo_diario, "delete");
    assert!(!lidos.chaves_estrangeiras);
    assert_eq!(lidos.sincronizacao, 1, "NORMAL");
    assert_eq!(lidos.espera_ocupado_ms, 30_000);
    assert!(!lidos.gatilhos_recursivos);
}

#[test]
fn chaves_estrangeiras_ligadas_recusam_filho_sem_pai() {
    let armazem = Armazem::em_memoria(&Pragmas::default()).expect("abrir");
    let c = armazem.conexao();
    c.execute_batch(
        "CREATE TABLE pai (id INTEGER PRIMARY KEY);
         CREATE TABLE filho (pai_id INTEGER NOT NULL REFERENCES pai(id));",
    )
    .expect("esquema");
    assert!(
        c.execute("INSERT INTO filho (pai_id) VALUES (99)", [])
            .is_err()
    );
}

#[test]
fn banco_novo_nasce_na_versao_zero_e_o_caminho_e_conhecido() {
    let pasta = PastaTemporaria::nova("versao0");
    let armazem = Armazem::abrir(pasta.banco(), &Pragmas::default()).expect("abrir");
    assert_eq!(armazem.versao().expect("versão"), 0);
    assert_eq!(armazem.caminho(), Some(pasta.banco().as_path()));
    let memoria = Armazem::em_memoria(&Pragmas::default()).expect("abrir");
    assert_eq!(memoria.caminho(), None);
}

#[test]
fn conexao_de_leitura_separada_ve_o_que_foi_confirmado_e_recusa_escrita() {
    let pasta = PastaTemporaria::nova("leitura");
    let armazem = Armazem::abrir(pasta.banco(), &Pragmas::default()).expect("abrir");
    armazem
        .conexao()
        .execute_batch("CREATE TABLE t (x INTEGER); INSERT INTO t VALUES (1);")
        .expect("esquema");

    let leitura = armazem.leitura().expect("leitura");
    let n: i64 = leitura
        .query_row("SELECT COUNT(*) FROM t", [], |l| l.get(0))
        .expect("contar");
    assert_eq!(n, 1);
    assert!(
        leitura.execute("INSERT INTO t VALUES (2)", []).is_err(),
        "a conexão de leitura não grava"
    );
}

#[test]
fn leitura_nao_bloqueia_durante_uma_escrita_e_so_ve_o_confirmado() {
    let pasta = PastaTemporaria::nova("leitura-wal");
    let mut escritor = Armazem::abrir(pasta.banco(), &Pragmas::default()).expect("abrir");
    escritor
        .conexao()
        .execute_batch("CREATE TABLE t (x INTEGER); INSERT INTO t VALUES (1);")
        .expect("esquema");
    let leitor = Armazem::abrir(pasta.banco(), &Pragmas::default()).expect("abrir");

    escritor
        .transacao_imediata(|tx| -> Result<(), Erro> {
            tx.execute("INSERT INTO t VALUES (2)", [])?;
            // Escrita ainda aberta: a leitura segue sem esperar e vê só o confirmado.
            let n: i64 = leitor
                .leitura()?
                .query_row("SELECT COUNT(*) FROM t", [], |l| l.get(0))?;
            assert_eq!(n, 1);
            Ok(())
        })
        .expect("transação");

    let n: i64 = leitor
        .leitura()
        .expect("leitura")
        .query_row("SELECT COUNT(*) FROM t", [], |l| l.get(0))
        .expect("contar");
    assert_eq!(n, 2);
}

#[test]
fn banco_em_memoria_nao_tem_conexao_de_leitura_separada() {
    let armazem = Armazem::em_memoria(&Pragmas::default()).expect("abrir");
    assert!(matches!(armazem.leitura(), Err(Erro::SemArquivo)));
}
