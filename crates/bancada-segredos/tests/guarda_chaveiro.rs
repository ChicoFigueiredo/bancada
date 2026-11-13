//! A guarda sobre o chaveiro do sistema (falso, em memória): o valor vai para lá, sob o nome do
//! serviço, e nunca para o disco; substituir, apagar, listar rótulos; falhas do chaveiro viram
//! erro tipado e o índice não finge o que não aconteceu.

mod comum;

use bancada_segredos::testes::ChaveiroEmMemoria;
use bancada_segredos::{
    ConfiguracaoDaGuarda, ErroDeSegredo, FalhaDoChaveiro, GuardaDeSegredos, ParametrosDeDerivacao,
    Situacao,
};
use comum::*;
use std::fs;
use std::sync::Arc;

#[test]
fn com_chaveiro_do_sistema_o_valor_vai_para_la_e_nunca_para_o_disco() {
    let (dir, chaveiro, guarda) = guarda_no_chaveiro("sistema");
    assert_eq!(guarda.situacao(), Situacao::ChaveiroDoSistema);

    guarda.gravar(ROTULO, segredo(VALOR)).expect("grava");

    assert_eq!(
        chaveiro.entradas(),
        vec![(SERVICO.to_owned(), ROTULO.to_owned())],
        "o nome do serviço é o da configuração e a conta é o rótulo"
    );
    assert_eq!(guarda.ler(ROTULO).expect("lê").expor(), VALOR);
    assert_eq!(guarda.rotulos(), vec![ROTULO.to_owned()]);
    assert!(dir.indice().exists());
    assert!(
        !dir.arquivo().exists(),
        "sem arquivo cifrado com o chaveiro"
    );
    assert!(arquivos_contendo(dir.caminho(), VALOR).is_empty());

    // Reaberta, o rótulo continua e o valor vem do chaveiro.
    let reaberta = abrir(dir.caminho(), &chaveiro);
    assert_eq!(reaberta.rotulos(), vec![ROTULO.to_owned()]);
    assert_eq!(reaberta.ler(ROTULO).expect("lê").expor(), VALOR);
}

#[test]
fn o_nome_do_servico_separa_os_apps() {
    let (dir, chaveiro, guarda) = guarda_no_chaveiro("servicos");
    guarda.gravar(ROTULO, segredo(VALOR)).expect("grava");
    let outro_dir = DirTemporario::novo("servicos-outro");
    let outro = GuardaDeSegredos::abrir(
        ConfiguracaoDaGuarda::nova("Outro App", outro_dir.caminho())
            .chaveiro(chaveiro.clone())
            .derivacao(ParametrosDeDerivacao::rapidos_para_testes()),
    )
    .expect("abre");

    assert_eq!(outro.ler(ROTULO).unwrap_err(), ErroDeSegredo::NaoEncontrado);
    outro
        .gravar(ROTULO, segredo("OUTRO_VALOR_FICTICIO"))
        .expect("grava");
    assert_eq!(guarda.ler(ROTULO).expect("lê").expor(), VALOR);
    assert_eq!(chaveiro.entradas().len(), 2);
    drop(dir);
}

#[test]
fn substituir_e_apagar() {
    let (_dir, chaveiro, guarda) = guarda_no_chaveiro("substitui");
    guarda.gravar(ROTULO, segredo(VALOR)).expect("grava");
    guarda
        .gravar(ROTULO, segredo("NOVO_VALOR_FICTICIO_99"))
        .expect("substitui");
    assert_eq!(
        guarda.ler(ROTULO).expect("lê").expor(),
        "NOVO_VALOR_FICTICIO_99"
    );
    assert_eq!(chaveiro.entradas().len(), 1);
    assert_eq!(guarda.rotulos().len(), 1);

    guarda.apagar(ROTULO).expect("apaga");
    assert!(chaveiro.entradas().is_empty());
    assert!(guarda.rotulos().is_empty());
    assert_eq!(
        guarda.ler(ROTULO).unwrap_err(),
        ErroDeSegredo::NaoEncontrado
    );
    guarda.apagar(ROTULO).expect("apagar de novo não é erro");
}

#[test]
fn rotulos_saem_em_ordem_e_sem_valores() {
    let (_dir, _chaveiro, guarda) = guarda_no_chaveiro("lista");
    for rotulo in ["senha:b", "cpf", "senha:a"] {
        guarda.gravar(rotulo, segredo(VALOR)).expect("grava");
    }
    let rotulos = guarda.rotulos();
    assert_eq!(rotulos, vec!["cpf", "senha:a", "senha:b"]);
    assert!(!format!("{rotulos:?}").contains(VALOR));
}

#[test]
fn valor_vazio_e_rotulo_invalido_sao_recusados() {
    let (dir, chaveiro, guarda) = guarda_no_chaveiro("validacao");
    assert_eq!(
        guarda.gravar(ROTULO, segredo("")),
        Err(ErroDeSegredo::SegredoVazio)
    );
    let longo = "x".repeat(257);
    for rotulo in ["", "com\nquebra", "com\u{0}nulo", longo.as_str()] {
        assert_eq!(
            guarda.gravar(rotulo, segredo(VALOR)),
            Err(ErroDeSegredo::RotuloInvalido),
            "{rotulo:?}"
        );
        assert_eq!(
            guarda.ler(rotulo).unwrap_err(),
            ErroDeSegredo::RotuloInvalido
        );
        assert_eq!(guarda.apagar(rotulo), Err(ErroDeSegredo::RotuloInvalido));
    }
    assert!(chaveiro.entradas().is_empty());
    assert!(guarda.rotulos().is_empty());
    assert!(arquivos(dir.caminho()).is_empty(), "nada foi gravado");
}

#[test]
fn chaveiro_que_falha_vira_erro_tipado_e_o_indice_nao_finge() {
    let (_dir, chaveiro, guarda) = guarda_no_chaveiro("falha");
    guarda.gravar(ROTULO, segredo(VALOR)).expect("grava");
    chaveiro.falhar(true);

    let erro = guarda.apagar(ROTULO).unwrap_err();
    assert_eq!(
        erro,
        ErroDeSegredo::ChaveiroDoSistema(FalhaDoChaveiro::Outra)
    );
    assert_eq!(
        guarda.rotulos(),
        vec![ROTULO.to_owned()],
        "não fingiu que apagou"
    );

    let erro = guarda.gravar("cpf", segredo(VALOR)).unwrap_err();
    assert!(
        matches!(erro, ErroDeSegredo::ChaveiroDoSistema(_)),
        "{erro:?}"
    );
    assert!(!guarda.rotulos().contains(&"cpf".to_owned()));
    assert!(matches!(
        guarda.ler(ROTULO),
        Err(ErroDeSegredo::ChaveiroDoSistema(_))
    ));
    assert!(!erro.to_string().contains(VALOR));

    chaveiro.falhar(false);
    guarda
        .apagar(ROTULO)
        .expect("com o chaveiro de volta, apaga");
    assert!(chaveiro.entradas().is_empty());
}

#[test]
fn operacoes_do_arquivo_cifrado_pedem_o_arquivo() {
    let (dir, _chaveiro, guarda) = guarda_no_chaveiro("sem-arquivo");
    assert_eq!(
        guarda.criar_arquivo(&segredo(SENHA)),
        Err(ErroDeSegredo::NaoPodeCriarArquivo)
    );
    assert_eq!(
        guarda.destrancar(&segredo(SENHA)),
        Err(ErroDeSegredo::SemArquivoCifrado)
    );
    assert_eq!(
        guarda.trocar_senha_mestra(&segredo(SENHA), &segredo("outra-senha-boa")),
        Err(ErroDeSegredo::SemArquivoCifrado)
    );
    guarda.trancar();
    assert_eq!(guarda.situacao(), Situacao::ChaveiroDoSistema);
    assert!(!dir.arquivo().exists());
}

#[test]
fn indice_adulterado_ou_ilegivel_e_recusado_ao_abrir() {
    let (dir, chaveiro, guarda) = guarda_no_chaveiro("indice");
    guarda.gravar(ROTULO, segredo(VALOR)).expect("grava");
    drop(guarda);

    fs::write(dir.indice(), b"{ isto nao e json").expect("corrompe");
    let erro = GuardaDeSegredos::abrir(configuracao(dir.caminho(), &chaveiro)).unwrap_err();
    assert_eq!(erro, ErroDeSegredo::IndiceCorrompido);

    fs::write(dir.indice(), br#"{"formato":1,"rotulos":["com\nquebra"]}"#).expect("adultera");
    let erro = GuardaDeSegredos::abrir(configuracao(dir.caminho(), &chaveiro)).unwrap_err();
    assert_eq!(erro, ErroDeSegredo::IndiceCorrompido);

    fs::write(dir.indice(), br#"{"formato":7,"rotulos":[]}"#).expect("versão nova");
    let erro = GuardaDeSegredos::abrir(configuracao(dir.caminho(), &chaveiro)).unwrap_err();
    assert_eq!(erro, ErroDeSegredo::FormatoNaoSuportado { versao: 7 });
}

#[test]
fn ler_vai_ao_chaveiro_mesmo_sem_o_rotulo_no_indice() {
    // O índice só lista: quem guarda o valor é o chaveiro. Assim o Chess Studio lê as contas
    // que o chess-vault gravou ("lichess/1/7") sem converter nada.
    let (_dir, chaveiro, guarda) = guarda_no_chaveiro("direto");
    chaveiro.semear(SERVICO, "lichess/1/7", "lip_FICTICIO_DO_CHESS_VAULT");
    assert!(guarda.rotulos().is_empty());
    assert_eq!(
        guarda.ler("lichess/1/7").expect("lê").expor(),
        "lip_FICTICIO_DO_CHESS_VAULT"
    );
}

#[test]
fn falha_de_disco_ao_abrir_e_erro_tipado() {
    let chaveiro = Arc::new(ChaveiroEmMemoria::novo());

    // O diretório da guarda é, na verdade, um arquivo.
    let base = DirTemporario::novo("disco");
    fs::create_dir_all(base.caminho()).expect("base");
    let como_arquivo = base.caminho().join("nao-sou-diretorio");
    fs::write(&como_arquivo, b"x").expect("arquivo");
    let erro = GuardaDeSegredos::abrir(configuracao(&como_arquivo, &chaveiro)).unwrap_err();
    assert!(matches!(erro, ErroDeSegredo::Disco { .. }), "{erro:?}");

    // O índice é um diretório.
    let dir = DirTemporario::novo("disco-indice");
    fs::create_dir_all(dir.indice()).expect("diretório no lugar do índice");
    let erro = GuardaDeSegredos::abrir(configuracao(dir.caminho(), &chaveiro)).unwrap_err();
    assert!(matches!(erro, ErroDeSegredo::Disco { .. }), "{erro:?}");
}

#[test]
fn a_guarda_e_compartilhada_entre_clones_e_threads() {
    let (_dir, _chaveiro, guarda) = guarda_no_chaveiro("threads");
    let clone = guarda.clone();
    std::thread::spawn(move || clone.gravar(ROTULO, segredo(VALOR)).expect("grava"))
        .join()
        .expect("thread");
    assert_eq!(guarda.ler(ROTULO).expect("lê").expor(), VALOR);
}
