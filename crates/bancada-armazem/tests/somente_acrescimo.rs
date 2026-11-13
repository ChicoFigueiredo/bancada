//! Tabela só de acréscimo: UPDATE e DELETE recusados por trigger.

mod common;

use bancada_armazem::{
    Armazem, Erro, Escada, Migracao, Pragmas, esta_protegida, gerar_sql_somente_acrescimo,
    instalar_somente_acrescimo,
};

fn com_decisao() -> Armazem {
    let a = Armazem::em_memoria(&Pragmas::default()).expect("abrir");
    a.conexao()
        .execute_batch(
            "CREATE TABLE decisao (id INTEGER PRIMARY KEY, motivo TEXT);
             INSERT INTO decisao (motivo) VALUES ('primeira');",
        )
        .expect("esquema");
    a
}

#[test]
fn trigger_recusa_update_e_delete_mas_aceita_insert() {
    let a = com_decisao();
    instalar_somente_acrescimo(a.conexao(), "decisao").expect("instalar");

    let c = a.conexao();
    c.execute("INSERT INTO decisao (motivo) VALUES ('segunda')", [])
        .expect("INSERT continua valendo");

    let upd = c
        .execute("UPDATE decisao SET motivo = 'adulterada'", [])
        .expect_err("UPDATE deve ser recusado");
    assert!(upd.to_string().contains("somente de acréscimo"), "{upd}");
    let del = c
        .execute("DELETE FROM decisao WHERE id = 1", [])
        .expect_err("DELETE deve ser recusado");
    assert!(del.to_string().contains("somente de acréscimo"), "{del}");

    let (n, motivo): (i64, String) = c
        .query_row("SELECT COUNT(*), MIN(motivo) FROM decisao", [], |l| {
            Ok((l.get(0)?, l.get(1)?))
        })
        .expect("ler");
    assert_eq!(n, 2);
    assert_eq!(motivo, "primeira");
}

#[test]
fn upsert_e_replace_tambem_nao_burlam_a_protecao() {
    let a = com_decisao();
    instalar_somente_acrescimo(a.conexao(), "decisao").expect("instalar");
    let c = a.conexao();
    assert!(
        c.execute(
            "INSERT INTO decisao (id, motivo) VALUES (1, 'x')
             ON CONFLICT (id) DO UPDATE SET motivo = excluded.motivo",
            []
        )
        .is_err()
    );
    // REPLACE só apaga a linha antiga com `recursive_triggers`; mesmo assim o conflito de
    // chave primária é a barreira: a linha original não muda.
    let _ = c.execute(
        "INSERT OR REPLACE INTO decisao (id, motivo) VALUES (1, 'x')",
        [],
    );
    let motivo: String = c
        .query_row("SELECT motivo FROM decisao WHERE id = 1", [], |l| l.get(0))
        .expect("ler");
    assert_eq!(motivo, "primeira");
}

#[test]
fn instalar_duas_vezes_e_inofensivo_e_esta_protegida_informa() {
    let a = com_decisao();
    assert!(!esta_protegida(a.conexao(), "decisao").expect("consultar"));
    instalar_somente_acrescimo(a.conexao(), "decisao").expect("instalar");
    instalar_somente_acrescimo(a.conexao(), "decisao").expect("instalar de novo");
    assert!(esta_protegida(a.conexao(), "decisao").expect("consultar"));
}

#[test]
fn sql_gerado_serve_para_dentro_de_uma_migracao() {
    let sql = gerar_sql_somente_acrescimo("decisao").expect("gerar");
    assert!(sql.contains("BEFORE UPDATE ON \"decisao\""));
    assert!(sql.contains("BEFORE DELETE ON \"decisao\""));

    fn migrar_decisao(tx: &rusqlite::Transaction<'_>) -> rusqlite::Result<()> {
        tx.execute_batch("CREATE TABLE decisao (id INTEGER PRIMARY KEY, motivo TEXT);")?;
        tx.execute_batch(&gerar_sql_somente_acrescimo("decisao").expect("gerar"))
    }
    let escada = [Migracao::codigo("decisao só de acréscimo", migrar_decisao)];
    let mut a = Armazem::em_memoria(&Pragmas::default()).expect("abrir");
    a.migrar(&Escada::nova(&escada)).expect("migrar");
    a.conexao()
        .execute("INSERT INTO decisao (motivo) VALUES ('a')", [])
        .expect("inserir");
    assert!(a.conexao().execute("DELETE FROM decisao", []).is_err());
}

#[test]
fn nome_que_nao_e_identificador_simples_e_recusado() {
    for ruim in [
        "",
        "1tabela",
        "a b",
        "decisao\"; DROP TABLE x; --",
        "dec-isao",
        "decisão",
    ] {
        assert!(
            matches!(
                gerar_sql_somente_acrescimo(ruim),
                Err(Erro::NomeInvalido(_))
            ),
            "{ruim:?} deveria ser recusado"
        );
    }
    let a = com_decisao();
    assert!(matches!(
        instalar_somente_acrescimo(a.conexao(), "a b"),
        Err(Erro::NomeInvalido(_))
    ));
}

#[test]
fn tabela_inexistente_falha_ao_instalar() {
    let a = Armazem::em_memoria(&Pragmas::default()).expect("abrir");
    assert!(instalar_somente_acrescimo(a.conexao(), "nao_existe").is_err());
}
