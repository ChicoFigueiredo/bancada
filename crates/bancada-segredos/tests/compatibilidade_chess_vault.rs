//! Leitura do arquivo do `chess-vault` (formato 1, assinatura `CSVAULT\0`) sem conversão: o
//! Chess Studio aponta a guarda para o `own.vault` que já tem e destranca com a senha de sempre.
//!
//! # De onde vem a fixture
//!
//! `tests/fixtures/chess-vault-v1.vault` foi gerada pelo **próprio código do chess-vault**
//! (`/mnt/d/Chico/chess-studio/crates/chess-vault`, sem alterar aquele repositório), por um
//! programa descartável que dependia dele por caminho:
//!
//! ```text
//! let config = VaultConfig::new(&dir)
//!     .keyring(Arc::new(MemoryKeyring::unavailable()))
//!     .kdf(KdfParams { memory_kib: 64, iterations: 1, parallelism: 1 });
//! let vault = SecretVault::open(config)?;
//! vault.create_own(&Secret::new("senha-mestra-ficticia"))?;
//! vault.store(LocalProfileId(1), ProviderId::Lichess, ConnectionId(7),
//!             Secret::new("lip_FICTICIO_DE_TESTE_42"))?;
//! vault.store(LocalProfileId(2), ProviderId::PlayStrategy, ConnectionId(9),
//!             Secret::new("ps_FICTICIO \"aspas\" \\ ção"))?;
//! // copia dir/own.vault para chess-vault-v1.vault
//! ```
//!
//! SHA-256: `b8cace21334140eb5365e48bb4dd2850bd7a1cfb5ba2152dbc10f9ac52852534`. Valores e
//! senha fictícios.

mod comum;

use bancada_segredos::testes::ChaveiroEmMemoria;
use bancada_segredos::{ErroDeSegredo, GuardaDeSegredos, Situacao};
use comum::*;
use std::fs;
use std::sync::Arc;

const FIXTURE: &str = "tests/fixtures/chess-vault-v1.vault";
const SENHA_DO_CHESS: &str = "senha-mestra-ficticia";
const TOKEN_LICHESS: &str = "lip_FICTICIO_DE_TESTE_42";
const TOKEN_PLAYSTRATEGY: &str = "ps_FICTICIO \"aspas\" \\ ção";

/// Um diretório com a fixture no nome que o chess-vault usa (`own.vault`).
fn guarda_com_a_fixture(nome: &str) -> (DirTemporario, Arc<ChaveiroEmMemoria>, GuardaDeSegredos) {
    let dir = DirTemporario::novo(nome);
    fs::create_dir_all(dir.caminho()).expect("diretório");
    fs::copy(FIXTURE, dir.caminho().join("own.vault")).expect("copia a fixture");
    let chaveiro = Arc::new(ChaveiroEmMemoria::indisponivel());
    let guarda = GuardaDeSegredos::abrir(
        configuracao(dir.caminho(), &chaveiro).arquivo_cifrado("own.vault"),
    )
    .expect("abre");
    (dir, chaveiro, guarda)
}

#[test]
fn a_fixture_e_do_formato_1_do_chess_vault() {
    let bytes = fs::read(FIXTURE).expect("fixture");
    assert!(bytes.starts_with(b"CSVAULT\0"));
    assert_eq!(&bytes[8..10], &1u16.to_le_bytes());
    assert!(!contem(&bytes, TOKEN_LICHESS.as_bytes()));
}

#[test]
fn le_o_arquivo_do_chess_vault_sem_converter() {
    let (_dir, _chaveiro, guarda) = guarda_com_a_fixture("v1-le");
    assert_eq!(guarda.situacao(), Situacao::ArquivoTrancado);
    assert_eq!(
        guarda.destrancar(&segredo("senha-errada-123")),
        Err(ErroDeSegredo::SenhaOuArquivoInvalido)
    );
    guarda
        .destrancar(&segredo(SENHA_DO_CHESS))
        .expect("destranca");

    // O rótulo é a conta que o chess-vault usava no keyring: provedor/perfil/conexão.
    assert_eq!(
        guarda.rotulos(),
        vec!["lichess/1/7".to_owned(), "playstrategy/2/9".to_owned()]
    );
    assert_eq!(
        guarda.ler("lichess/1/7").expect("lê").expor(),
        TOKEN_LICHESS
    );
    assert_eq!(
        guarda.ler("playstrategy/2/9").expect("lê").expor(),
        TOKEN_PLAYSTRATEGY
    );
}

#[test]
fn so_ler_nao_regrava_o_arquivo_do_chess_vault() {
    let (dir, _chaveiro, guarda) = guarda_com_a_fixture("v1-so-le");
    guarda
        .destrancar(&segredo(SENHA_DO_CHESS))
        .expect("destranca");
    let _ = guarda.ler("lichess/1/7").expect("lê");
    assert_eq!(
        fs::read(dir.caminho().join("own.vault")).expect("arquivo"),
        fs::read(FIXTURE).expect("fixture"),
        "o Chess Studio antigo ainda consegue abrir"
    );
}

#[test]
fn a_primeira_gravacao_passa_para_o_formato_2_e_mantem_tudo() {
    let (dir, chaveiro, guarda) = guarda_com_a_fixture("v1-migra");
    guarda
        .destrancar(&segredo(SENHA_DO_CHESS))
        .expect("destranca");
    guarda.gravar("cpf", segredo(VALOR)).expect("grava");

    let bytes = fs::read(dir.caminho().join("own.vault")).expect("arquivo");
    assert!(bytes.starts_with(b"CSVAULT\0"));
    assert_eq!(&bytes[8..10], &2u16.to_le_bytes(), "agora no formato 2");
    assert!(!contem(&bytes, VALOR.as_bytes()));
    drop(guarda);

    let guarda = GuardaDeSegredos::abrir(
        configuracao(dir.caminho(), &chaveiro).arquivo_cifrado("own.vault"),
    )
    .expect("reabre");
    guarda
        .destrancar(&segredo(SENHA_DO_CHESS))
        .expect("a mesma senha vale");
    assert_eq!(
        guarda.rotulos(),
        vec![
            "cpf".to_owned(),
            "lichess/1/7".to_owned(),
            "playstrategy/2/9".to_owned()
        ]
    );
    assert_eq!(
        guarda.ler("lichess/1/7").expect("lê").expor(),
        TOKEN_LICHESS
    );
    assert_eq!(guarda.ler("cpf").expect("lê").expor(), VALOR);
}

#[test]
fn trocar_a_senha_de_um_arquivo_do_chess_vault() {
    let (dir, chaveiro, guarda) = guarda_com_a_fixture("v1-troca");
    guarda
        .trocar_senha_mestra(&segredo(SENHA_DO_CHESS), &segredo(SENHA))
        .expect("troca");
    drop(guarda);
    let guarda = GuardaDeSegredos::abrir(
        configuracao(dir.caminho(), &chaveiro).arquivo_cifrado("own.vault"),
    )
    .expect("reabre");
    guarda.destrancar(&segredo(SENHA)).expect("destranca");
    assert_eq!(
        guarda.ler("playstrategy/2/9").expect("lê").expor(),
        TOKEN_PLAYSTRATEGY
    );
}
