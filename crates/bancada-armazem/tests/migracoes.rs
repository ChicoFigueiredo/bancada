//! Escada de migrações por `PRAGMA user_version`.

mod common;

use bancada_armazem::{Armazem, Erro, Escada, LinhaDeBase, Migracao, Pragmas};
use common::PastaTemporaria;
use rusqlite::Connection;

const V1: Migracao = Migracao::sql(
    "pessoa",
    "CREATE TABLE pessoa (id INTEGER PRIMARY KEY, nome TEXT NOT NULL);",
);
const V2: Migracao = Migracao::sql("apelido", "ALTER TABLE pessoa ADD COLUMN apelido TEXT;");
const V3: Migracao = Migracao::sql("indice", "CREATE INDEX pessoa_nome ON pessoa (nome);");
const ESCADA: &[Migracao] = &[V1, V2, V3];

fn abrir(pasta: &PastaTemporaria) -> Armazem {
    Armazem::abrir(pasta.banco(), &Pragmas::default()).expect("abrir")
}

fn colunas(c: &Connection, tabela: &str) -> Vec<String> {
    let mut s = c
        .prepare(&format!("SELECT name FROM pragma_table_info('{tabela}')"))
        .expect("preparar");
    s.query_map([], |l| l.get::<_, String>(0))
        .expect("consultar")
        .collect::<Result<_, _>>()
        .expect("colunas")
}

#[test]
fn banco_novo_recebe_todas_as_migracoes_em_ordem() {
    let pasta = PastaTemporaria::nova("novo");
    let mut a = abrir(&pasta);
    let relatorio = a.migrar(&Escada::nova(ESCADA)).expect("migrar");
    assert_eq!(relatorio.versao_inicial, 0);
    assert_eq!(relatorio.versao_final, 3);
    assert!(!relatorio.linha_de_base_carimbada);
    assert_eq!(a.versao().expect("versão"), 3);
    assert_eq!(colunas(a.conexao(), "pessoa"), ["id", "nome", "apelido"]);
}

#[test]
fn migrar_de_novo_nao_faz_nada() {
    let pasta = PastaTemporaria::nova("idempotente");
    let mut a = abrir(&pasta);
    a.migrar(&Escada::nova(ESCADA)).expect("migrar");
    let relatorio = a.migrar(&Escada::nova(ESCADA)).expect("migrar de novo");
    assert_eq!(relatorio.versao_inicial, 3);
    assert_eq!(relatorio.versao_final, 3);
}

#[test]
fn banco_da_versao_anterior_migra_e_preserva_os_dados() {
    let pasta = PastaTemporaria::nova("anterior");
    {
        let mut a = abrir(&pasta);
        a.migrar_ate(&Escada::nova(ESCADA), 2)
            .expect("migrar até 2");
        assert_eq!(a.versao().expect("versão"), 2);
        a.conexao()
            .execute(
                "INSERT INTO pessoa (nome, apelido) VALUES ('Ana', 'Aninha')",
                [],
            )
            .expect("inserir");
    }
    let mut a = abrir(&pasta);
    a.migrar(&Escada::nova(ESCADA)).expect("migrar");
    assert_eq!(a.versao().expect("versão"), 3);
    let apelido: String = a
        .conexao()
        .query_row("SELECT apelido FROM pessoa WHERE nome = 'Ana'", [], |l| {
            l.get(0)
        })
        .expect("ler");
    assert_eq!(apelido, "Aninha");
}

#[test]
fn banco_mais_novo_que_o_app_e_recusado_sem_tocar_nele() {
    let pasta = PastaTemporaria::nova("mais-novo");
    {
        let mut a = abrir(&pasta);
        a.migrar(&Escada::nova(ESCADA)).expect("migrar");
        a.conexao()
            .pragma_update(None, "user_version", 4)
            .expect("simular app mais novo");
    }
    let mut a = abrir(&pasta);
    let erro = a.migrar(&Escada::nova(ESCADA)).expect_err("deve recusar");
    assert!(matches!(
        erro,
        Erro::BancoMaisNovo {
            encontrada: 4,
            suportada: 3
        }
    ));
    assert!(erro.to_string().contains("atualize"));
    assert_eq!(a.versao().expect("versão"), 4, "o banco não foi alterado");
}

#[test]
fn migracao_que_falha_e_desfeita_e_mantem_a_versao_anterior() {
    let mut a = Armazem::em_memoria(&Pragmas::default()).expect("abrir");
    a.migrar_ate(&Escada::nova(ESCADA), 1)
        .expect("migrar até 1");
    // Sabota a migração 3: já existe um índice homônimo, o DDL falha.
    a.conexao()
        .execute_batch("CREATE INDEX pessoa_nome ON pessoa (id)")
        .expect("sabotar");
    // E a 2 passa, mas a 3 falha: a versão fica em 2, não em 1 nem em 3.
    let erro = a
        .migrar(&Escada::nova(ESCADA))
        .expect_err("a 3 deve falhar");
    assert!(
        matches!(erro, Erro::MigracaoFalhou { versao: 3, .. }),
        "{erro}"
    );
    assert_eq!(a.versao().expect("versão"), 2);
}

#[test]
fn migracao_parcial_nao_deixa_rastro() {
    const RUIM: Migracao = Migracao::sql(
        "metade boa, metade ruim",
        "CREATE TABLE metade (x INTEGER); CREATE TABLE pessoa (x INTEGER);",
    );
    let mut a = Armazem::em_memoria(&Pragmas::default()).expect("abrir");
    let escada = [V1, RUIM];
    let erro = a.migrar(&Escada::nova(&escada)).expect_err("deve falhar");
    assert!(matches!(erro, Erro::MigracaoFalhou { versao: 2, .. }));
    assert_eq!(a.versao().expect("versão"), 1);
    let sobras: i64 = a
        .conexao()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name = 'metade'",
            [],
            |l| l.get(0),
        )
        .expect("contar");
    assert_eq!(sobras, 0, "a migração parcial foi desfeita");
}

#[test]
fn migracao_em_codigo_roda_na_mesma_transacao() {
    fn popular(tx: &rusqlite::Transaction<'_>) -> rusqlite::Result<()> {
        tx.execute("INSERT INTO pessoa (nome) VALUES ('semente')", [])?;
        Ok(())
    }
    fn falhar(tx: &rusqlite::Transaction<'_>) -> rusqlite::Result<()> {
        tx.execute("INSERT INTO pessoa (nome) VALUES ('nao deve ficar')", [])?;
        tx.execute("INSERT INTO nao_existe VALUES (1)", [])?;
        Ok(())
    }
    let escada = [
        V1,
        Migracao::codigo("semente", popular),
        Migracao::codigo("falha", falhar),
    ];
    let mut a = Armazem::em_memoria(&Pragmas::default()).expect("abrir");
    assert!(a.migrar(&Escada::nova(&escada)).is_err());
    assert_eq!(a.versao().expect("versão"), 2);
    let nomes: Vec<String> = a
        .conexao()
        .prepare("SELECT nome FROM pessoa")
        .expect("preparar")
        .query_map([], |l| l.get(0))
        .expect("consultar")
        .collect::<Result<_, _>>()
        .expect("nomes");
    assert_eq!(nomes, ["semente"]);
}

#[test]
fn pendentes_lista_o_que_vai_mudar_antes_de_migrar() {
    let mut a = Armazem::em_memoria(&Pragmas::default()).expect("abrir");
    let escada = Escada::nova(ESCADA);
    let descricoes = |p: &[Migracao]| p.iter().map(|m| m.descricao).collect::<Vec<_>>();
    assert_eq!(
        descricoes(a.pendentes(&escada).expect("pendentes")),
        ["pessoa", "apelido", "indice"]
    );
    a.migrar_ate(&escada, 2).expect("migrar até 2");
    assert_eq!(
        descricoes(a.pendentes(&escada).expect("pendentes")),
        ["indice"]
    );
    a.migrar(&escada).expect("migrar");
    assert!(a.pendentes(&escada).expect("pendentes").is_empty());
}

// ---- Linha de base: adotar um banco criado por outro sistema (o inventario.sqlite do Bun) ----

const BASE_V1: Migracao = Migracao::sql(
    "esquema do Bun (versão 2), adotado",
    "CREATE TABLE IF NOT EXISTS meta (chave TEXT PRIMARY KEY, valor TEXT);
     CREATE TABLE IF NOT EXISTS achado (id INTEGER PRIMARY KEY);",
);
const BASE_V2: Migracao =
    Migracao::sql("decisao", "CREATE TABLE decisao (id INTEGER PRIMARY KEY);");
const TABELAS: &[&str] = &["meta", "achado"];

fn escada_com_base() -> Escada<'static> {
    Escada::nova(&[BASE_V1, BASE_V2]).com_linha_de_base(
        LinhaDeBase::nova(1, TABELAS)
            .com_marcador("SELECT valor FROM meta WHERE chave = 'schema_versao'", "2"),
    )
}

/// Banco "como o Bun o deixou": tabelas criadas, `user_version` zerado.
fn banco_do_bun(pasta: &PastaTemporaria, versao_meta: Option<&str>, com_achado: bool) {
    let c = Connection::open(pasta.banco()).expect("abrir cru");
    c.execute_batch("CREATE TABLE meta (chave TEXT PRIMARY KEY, valor TEXT);")
        .expect("meta");
    if com_achado {
        c.execute_batch(
            "CREATE TABLE achado (id INTEGER PRIMARY KEY); INSERT INTO achado VALUES (7);",
        )
        .expect("achado");
    }
    if let Some(v) = versao_meta {
        c.execute("INSERT INTO meta VALUES ('schema_versao', ?1)", [v])
            .expect("marcador");
    }
}

#[test]
fn linha_de_base_so_carimba_quando_as_tabelas_existem_e_depois_migra() {
    let pasta = PastaTemporaria::nova("base-ok");
    banco_do_bun(&pasta, Some("2"), true);
    let mut a = abrir(&pasta);
    assert_eq!(a.versao().expect("versão"), 0);
    assert_eq!(a.pendentes(&escada_com_base()).expect("pendentes").len(), 1);

    let relatorio = a.migrar(&escada_com_base()).expect("migrar");
    assert!(relatorio.linha_de_base_carimbada);
    assert_eq!(relatorio.versao_inicial, 0);
    assert_eq!(relatorio.versao_final, 2);
    // A migração 1 NÃO rodou de novo (os dados do Bun estão intactos) e a 2 foi aplicada.
    let n: i64 = a
        .conexao()
        .query_row("SELECT COUNT(*) FROM achado", [], |l| l.get(0))
        .expect("achados");
    assert_eq!(n, 1);
    a.conexao()
        .query_row("SELECT COUNT(*) FROM decisao", [], |l| l.get::<_, i64>(0))
        .expect("a migração 2 criou decisao");
}

#[test]
fn linha_de_base_recusa_e_nao_carimba_quando_faltam_tabelas() {
    let pasta = PastaTemporaria::nova("base-falta");
    banco_do_bun(&pasta, Some("2"), false);
    let mut a = abrir(&pasta);
    let erro = a.migrar(&escada_com_base()).expect_err("deve recusar");
    match erro {
        Erro::LinhaDeBaseSemTabelas { faltando } => assert_eq!(faltando, ["achado"]),
        outro => panic!("erro inesperado: {outro}"),
    }
    assert_eq!(a.versao().expect("versão"), 0, "nada foi carimbado");
}

#[test]
fn linha_de_base_recusa_quando_o_marcador_nao_bate() {
    let pasta = PastaTemporaria::nova("base-marcador");
    banco_do_bun(&pasta, Some("1"), true);
    let mut a = abrir(&pasta);
    let erro = a.migrar(&escada_com_base()).expect_err("deve recusar");
    assert!(
        matches!(
            &erro,
            Erro::LinhaDeBaseMarcadorDiferente { esperado, encontrado: Some(e) } if esperado == "2" && e == "1"
        ),
        "{erro}"
    );
    assert_eq!(a.versao().expect("versão"), 0);

    // Sem a linha do marcador, também recusa.
    let pasta = PastaTemporaria::nova("base-sem-marcador");
    banco_do_bun(&pasta, None, true);
    let mut a = abrir(&pasta);
    let erro = a.migrar(&escada_com_base()).expect_err("deve recusar");
    assert!(matches!(
        erro,
        Erro::LinhaDeBaseMarcadorDiferente {
            encontrado: None,
            ..
        }
    ));
    assert_eq!(a.versao().expect("versão"), 0);
}

#[test]
fn linha_de_base_nao_interfere_em_banco_vazio_nem_em_banco_ja_versionado() {
    // Vazio: roda a migração 1 normalmente.
    let mut a = Armazem::em_memoria(&Pragmas::default()).expect("abrir");
    let relatorio = a.migrar(&escada_com_base()).expect("migrar");
    assert!(!relatorio.linha_de_base_carimbada);
    assert_eq!(relatorio.versao_final, 2);

    // Já versionado: segue a escada, sem checar a base de novo.
    let relatorio = a.migrar(&escada_com_base()).expect("migrar de novo");
    assert!(!relatorio.linha_de_base_carimbada);
    assert_eq!(relatorio.versao_inicial, 2);
}

#[test]
fn linha_de_base_alem_da_escada_e_erro_de_montagem() {
    let mut a = Armazem::em_memoria(&Pragmas::default()).expect("abrir");
    a.conexao()
        .execute_batch("CREATE TABLE x (y)")
        .expect("tabela");
    let escada = Escada::nova(&[V1]).com_linha_de_base(LinhaDeBase::nova(5, &["x"]));
    assert!(matches!(
        a.migrar(&escada),
        Err(Erro::EscadaInconsistente(_))
    ));
    assert_eq!(a.versao().expect("versão"), 0);
}

#[test]
fn dois_armazens_migrando_o_mesmo_arquivo_aplicam_cada_migracao_uma_vez() {
    let pasta = PastaTemporaria::nova("corrida");
    let caminho = pasta.banco();
    let fios: Vec<_> = (0..4)
        .map(|_| {
            let caminho = caminho.clone();
            std::thread::spawn(move || {
                let mut a = Armazem::abrir(&caminho, &Pragmas::default()).expect("abrir");
                a.migrar(&Escada::nova(ESCADA)).expect("migrar");
            })
        })
        .collect();
    for f in fios {
        f.join().expect("fio");
    }
    let a = Armazem::abrir(&caminho, &Pragmas::default()).expect("abrir");
    assert_eq!(a.versao().expect("versão"), 3);
}
