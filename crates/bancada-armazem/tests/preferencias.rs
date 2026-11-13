//! Preferências chave -> JSON, com escopo opcional (por exemplo, o perfil).

mod common;

use bancada_armazem::{
    Armazem, ESCOPO_GLOBAL, Erro, Escada, MIGRACAO_PREFERENCIAS, Pragmas, gravar_preferencia,
    ler_preferencia, remover_preferencia,
};
use common::PastaTemporaria;
use serde_json::{Value, json};

fn armazem(pasta: &PastaTemporaria) -> Armazem {
    let mut a = Armazem::abrir(pasta.banco(), &Pragmas::default()).expect("abrir");
    a.migrar(&Escada::nova(&[MIGRACAO_PREFERENCIAS]))
        .expect("migrar");
    a
}

#[test]
fn preferencia_nunca_gravada_e_none() {
    let pasta = PastaTemporaria::nova("pref-nenhuma");
    let a = armazem(&pasta);
    let v: Option<Value> = ler_preferencia(a.conexao(), ESCOPO_GLOBAL, "tema").expect("ler");
    assert_eq!(v, None);
}

#[test]
fn preferencia_persiste_entre_aberturas() {
    let pasta = PastaTemporaria::nova("pref-persiste");
    {
        let a = armazem(&pasta);
        gravar_preferencia(
            a.conexao(),
            ESCOPO_GLOBAL,
            "tabuleiro.vista",
            &json!({"camera": "preto"}),
        )
        .expect("gravar");
    }
    let a = armazem(&pasta);
    let v: Option<Value> =
        ler_preferencia(a.conexao(), ESCOPO_GLOBAL, "tabuleiro.vista").expect("ler");
    assert_eq!(v, Some(json!({"camera": "preto"})));
}

#[test]
fn gravar_de_novo_substitui_o_valor() {
    let pasta = PastaTemporaria::nova("pref-substitui");
    let a = armazem(&pasta);
    gravar_preferencia(a.conexao(), ESCOPO_GLOBAL, "k", &1).expect("gravar");
    gravar_preferencia(a.conexao(), ESCOPO_GLOBAL, "k", &2).expect("gravar");
    assert_eq!(
        ler_preferencia::<i64>(a.conexao(), ESCOPO_GLOBAL, "k").expect("ler"),
        Some(2)
    );
}

#[test]
fn preferencias_sao_por_escopo() {
    let pasta = PastaTemporaria::nova("pref-escopo");
    let a = armazem(&pasta);
    gravar_preferencia(a.conexao(), "perfil:1", "tema", &"escuro").expect("gravar");
    gravar_preferencia(a.conexao(), "perfil:2", "tema", &"claro").expect("gravar");
    assert_eq!(
        ler_preferencia::<String>(a.conexao(), "perfil:1", "tema").expect("ler"),
        Some("escuro".to_owned())
    );
    assert_eq!(
        ler_preferencia::<String>(a.conexao(), "perfil:2", "tema").expect("ler"),
        Some("claro".to_owned())
    );
    assert_eq!(
        ler_preferencia::<String>(a.conexao(), ESCOPO_GLOBAL, "tema").expect("ler"),
        None
    );
}

#[test]
fn remover_informa_se_havia_o_que_remover() {
    let pasta = PastaTemporaria::nova("pref-remove");
    let a = armazem(&pasta);
    gravar_preferencia(a.conexao(), ESCOPO_GLOBAL, "k", &true).expect("gravar");
    assert!(remover_preferencia(a.conexao(), ESCOPO_GLOBAL, "k").expect("remover"));
    assert!(!remover_preferencia(a.conexao(), ESCOPO_GLOBAL, "k").expect("remover de novo"));
    assert_eq!(
        ler_preferencia::<bool>(a.conexao(), ESCOPO_GLOBAL, "k").expect("ler"),
        None
    );
}

#[test]
fn json_que_nao_serve_para_o_tipo_pedido_e_erro_tipado() {
    let pasta = PastaTemporaria::nova("pref-tipo");
    let a = armazem(&pasta);
    gravar_preferencia(a.conexao(), ESCOPO_GLOBAL, "k", &"texto").expect("gravar");
    assert!(matches!(
        ler_preferencia::<i64>(a.conexao(), ESCOPO_GLOBAL, "k"),
        Err(Erro::Json(_))
    ));
}

#[test]
fn funciona_dentro_de_uma_transacao_imediata() {
    let pasta = PastaTemporaria::nova("pref-transacao");
    let mut a = armazem(&pasta);
    let r = a.transacao_imediata(|tx| -> Result<(), Erro> {
        gravar_preferencia(tx, ESCOPO_GLOBAL, "k", &1)?;
        Err(Erro::SemArquivo)
    });
    assert!(r.is_err());
    assert_eq!(
        ler_preferencia::<i64>(a.conexao(), ESCOPO_GLOBAL, "k").expect("ler"),
        None,
        "o rollback levou a preferência junto"
    );
}
