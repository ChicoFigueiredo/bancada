//! O keyring de verdade do sistema operacional. **Ignorados** por padrão: dependem do SO
//! (Secret Service, Credential Manager, Keychain) e podem abrir um diálogo de desbloqueio.
//! Rode à mão, numa sessão com keyring:
//!
//! ```text
//! cargo test -p bancada-segredos --test chaveiro_real -- --ignored
//! ```
//!
//! Usa um serviço próprio de teste e um valor fictício, e apaga o que gravou.

use bancada_segredos::{ChaveiroDoSistema, ChaveiroNativo, Segredo};

const SERVICO: &str = "bancada-segredos (teste)";
const ROTULO: &str = "teste:ida-e-volta";
const VALOR: &str = "VALOR_FICTICIO_DO_TESTE_DE_KEYRING";

#[test]
#[ignore = "toca o keyring real do sistema operacional"]
fn ida_e_volta_no_keyring_real() {
    let chaveiro = ChaveiroNativo;
    assert!(chaveiro.esta_disponivel(), "esta sessão não tem keyring");
    chaveiro
        .gravar(SERVICO, ROTULO, &Segredo::novo(VALOR))
        .expect("grava");
    let lido = chaveiro.ler(SERVICO, ROTULO).expect("lê").expect("existe");
    assert_eq!(lido.expor(), VALOR);
    chaveiro.apagar(SERVICO, ROTULO).expect("apaga");
    assert!(chaveiro.ler(SERVICO, ROTULO).expect("lê").is_none());
    chaveiro
        .apagar(SERVICO, ROTULO)
        .expect("apagar de novo não é erro");
}

#[test]
#[ignore = "consulta o keyring real do sistema operacional"]
fn disponibilidade_do_keyring_real_nao_trava() {
    // Só consulta a inicialização do keyring (sem gravar nem pedir desbloqueio).
    let _ = ChaveiroNativo.esta_disponivel();
}
