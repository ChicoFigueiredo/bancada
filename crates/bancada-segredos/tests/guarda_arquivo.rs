//! A guarda sem chaveiro do sistema: o arquivo cifrado por senha-mestra. Nada em texto puro,
//! trancar e destrancar, troca de senha, nenhuma troca de lugar em silêncio, permissões e
//! gravação atômica.

mod comum;

use bancada_segredos::testes::ChaveiroEmMemoria;
use bancada_segredos::{
    ErroDeSegredo, GuardaDeSegredos, ParametrosDeDerivacao, Situacao, TAMANHO_MINIMO_DA_SENHA,
};
use comum::*;
use std::fs;
use std::sync::Arc;

#[test]
fn sem_chaveiro_nada_e_guardado_ate_criar_o_arquivo() {
    let dir = DirTemporario::novo("sem-chaveiro");
    let chaveiro = Arc::new(ChaveiroEmMemoria::indisponivel());
    let guarda = abrir(dir.caminho(), &chaveiro);

    assert_eq!(guarda.situacao(), Situacao::Indisponivel);
    assert_eq!(
        guarda.gravar(ROTULO, segredo(VALOR)),
        Err(ErroDeSegredo::Indisponivel)
    );
    assert_eq!(guarda.ler(ROTULO).unwrap_err(), ErroDeSegredo::Indisponivel);
    assert!(
        arquivos(dir.caminho()).is_empty(),
        "{:?}",
        arquivos(dir.caminho())
    );
    assert!(chaveiro.entradas().is_empty());

    assert_eq!(
        guarda.criar_arquivo(&segredo("curta")),
        Err(ErroDeSegredo::SenhaFraca {
            minimo: TAMANHO_MINIMO_DA_SENHA
        })
    );
    guarda.criar_arquivo(&segredo(SENHA)).expect("cria");
    assert_eq!(guarda.situacao(), Situacao::ArquivoAberto);
    guarda.gravar(ROTULO, segredo(VALOR)).expect("grava");
    assert_eq!(guarda.ler(ROTULO).expect("lê").expor(), VALOR);

    let bytes = fs::read(dir.arquivo()).expect("arquivo cifrado");
    assert!(bytes.starts_with(b"CSVAULT\0"));
    assert_eq!(&bytes[8..10], &2u16.to_le_bytes(), "grava no formato 2");
    assert!(chaveiro.entradas().is_empty());
}

#[test]
fn reaberta_comeca_trancada_e_so_a_senha_certa_destranca() {
    let (dir, chaveiro, guarda) = guarda_em_arquivo("reabre");
    drop(guarda);

    let guarda = abrir(dir.caminho(), &chaveiro);
    assert_eq!(guarda.situacao(), Situacao::ArquivoTrancado);
    assert_eq!(guarda.rotulos(), vec![ROTULO.to_owned()], "lista trancada");
    assert_eq!(guarda.ler(ROTULO).unwrap_err(), ErroDeSegredo::Trancado);

    assert_eq!(
        guarda.destrancar(&segredo("senha-errada-123")),
        Err(ErroDeSegredo::SenhaOuArquivoInvalido)
    );
    assert_eq!(guarda.situacao(), Situacao::ArquivoTrancado);
    guarda.destrancar(&segredo(SENHA)).expect("destranca");
    assert_eq!(guarda.situacao(), Situacao::ArquivoAberto);
    assert_eq!(guarda.ler(ROTULO).expect("lê").expor(), VALOR);
}

#[test]
fn trancar_esquece_a_chave_e_os_valores() {
    let (_dir, _chaveiro, guarda) = guarda_em_arquivo("tranca");
    guarda.trancar();
    assert_eq!(guarda.situacao(), Situacao::ArquivoTrancado);
    assert_eq!(guarda.ler(ROTULO).unwrap_err(), ErroDeSegredo::Trancado);
    assert_eq!(
        guarda.gravar("cpf", segredo(VALOR)),
        Err(ErroDeSegredo::Trancado)
    );
    assert_eq!(guarda.apagar(ROTULO), Err(ErroDeSegredo::Trancado));
    assert_eq!(guarda.rotulos(), vec![ROTULO.to_owned()]);
}

#[test]
fn trocar_a_senha_mestra() {
    let (dir, chaveiro, guarda) = guarda_em_arquivo("troca-senha");
    let antes = fs::read(dir.arquivo()).expect("arquivo");
    let nova = "outra-senha-mestra-ficticia";

    assert_eq!(
        guarda.trocar_senha_mestra(&segredo("nao-e-esta-senha"), &segredo(nova)),
        Err(ErroDeSegredo::SenhaOuArquivoInvalido)
    );
    assert_eq!(
        guarda.trocar_senha_mestra(&segredo(SENHA), &segredo("curta")),
        Err(ErroDeSegredo::SenhaFraca {
            minimo: TAMANHO_MINIMO_DA_SENHA
        })
    );
    guarda
        .trocar_senha_mestra(&segredo(SENHA), &segredo(nova))
        .expect("troca");
    let depois = fs::read(dir.arquivo()).expect("arquivo");
    assert_ne!(&antes[23..39], &depois[23..39], "sal novo");
    assert_ne!(&antes[39..71], &depois[39..71], "verificador novo");
    drop(guarda);

    let guarda = abrir(dir.caminho(), &chaveiro);
    assert_eq!(
        guarda.destrancar(&segredo(SENHA)),
        Err(ErroDeSegredo::SenhaOuArquivoInvalido)
    );
    guarda.destrancar(&segredo(nova)).expect("a nova vale");
    assert_eq!(guarda.ler(ROTULO).expect("lê").expor(), VALOR);
    assert!(arquivos_contendo(dir.caminho(), nova).is_empty());

    // Trancada, a troca também funciona (lê o arquivo com a senha atual).
    guarda.trancar();
    guarda
        .trocar_senha_mestra(&segredo(nova), &segredo(SENHA))
        .expect("troca trancada");
    assert_eq!(guarda.situacao(), Situacao::ArquivoAberto);
    assert_eq!(guarda.ler(ROTULO).expect("lê").expor(), VALOR);
}

#[test]
fn chaveiro_que_aparece_depois_nao_assume_em_silencio() {
    let (dir, _indisponivel, guarda) = guarda_em_arquivo("sem-troca");
    drop(guarda);

    let sistema = Arc::new(ChaveiroEmMemoria::novo());
    let guarda = abrir(dir.caminho(), &sistema);
    assert_eq!(guarda.situacao(), Situacao::ArquivoTrancado);
    assert_eq!(
        guarda.criar_arquivo(&segredo(SENHA)),
        Err(ErroDeSegredo::NaoPodeCriarArquivo)
    );
    guarda.destrancar(&segredo(SENHA)).expect("destranca");
    guarda
        .gravar("cpf", segredo(VALOR))
        .expect("grava no arquivo");
    assert!(sistema.entradas().is_empty());
}

#[test]
fn criar_de_novo_nao_sobrescreve_o_arquivo() {
    let (dir, _chaveiro, guarda) = guarda_em_arquivo("cria-de-novo");
    let antes = fs::read(dir.arquivo()).expect("arquivo");
    assert_eq!(
        guarda.criar_arquivo(&segredo("outra-senha-qualquer")),
        Err(ErroDeSegredo::NaoPodeCriarArquivo)
    );
    assert_eq!(fs::read(dir.arquivo()).expect("arquivo"), antes);
}

#[test]
fn apagar_e_listar_com_o_arquivo_aberto() {
    let (dir, chaveiro, guarda) = guarda_em_arquivo("apaga");
    guarda
        .gravar("cpf", segredo("CPF_FICTICIO_000"))
        .expect("grava");
    assert_eq!(guarda.rotulos(), vec!["cpf".to_owned(), ROTULO.to_owned()]);

    guarda.apagar(ROTULO).expect("apaga");
    guarda.apagar(ROTULO).expect("apagar de novo não é erro");
    assert_eq!(guarda.rotulos(), vec!["cpf".to_owned()]);
    drop(guarda);

    let guarda = abrir(dir.caminho(), &chaveiro);
    assert_eq!(guarda.rotulos(), vec!["cpf".to_owned()]);
    guarda.destrancar(&segredo(SENHA)).expect("destranca");
    assert_eq!(
        guarda.ler(ROTULO).unwrap_err(),
        ErroDeSegredo::NaoEncontrado
    );
    assert_eq!(guarda.ler("cpf").expect("lê").expor(), "CPF_FICTICIO_000");
    assert!(arquivos_contendo(dir.caminho(), VALOR).is_empty());
}

#[test]
fn destrancar_corrige_o_indice_pelo_conteudo_cifrado() {
    let (dir, chaveiro, guarda) = guarda_em_arquivo("indice-corrige");
    drop(guarda);
    fs::write(
        dir.indice(),
        br#"{"formato":1,"rotulos":["inventado",  "senha:banco-ficticio"]}"#,
    )
    .expect("adultera o índice");

    let guarda = abrir(dir.caminho(), &chaveiro);
    assert_eq!(guarda.rotulos().len(), 2, "trancada, vale o índice");
    guarda.destrancar(&segredo(SENHA)).expect("destranca");
    assert_eq!(guarda.rotulos(), vec![ROTULO.to_owned()]);
    drop(guarda);
    let guarda = abrir(dir.caminho(), &chaveiro);
    assert_eq!(
        guarda.rotulos(),
        vec![ROTULO.to_owned()],
        "índice regravado"
    );
}

#[test]
fn parametros_fracos_demais_ou_absurdos_sao_recusados() {
    assert_eq!(
        ParametrosDeDerivacao::novos(8 * 1024, 1, 1),
        Err(ErroDeSegredo::ParametrosInvalidos)
    );
    assert_eq!(
        ParametrosDeDerivacao::novos(64 * 1024, 1, 1),
        Err(ErroDeSegredo::ParametrosInvalidos)
    );
    assert_eq!(
        ParametrosDeDerivacao::novos(u32::MAX, 3, 1),
        Err(ErroDeSegredo::ParametrosInvalidos)
    );
    assert_eq!(
        ParametrosDeDerivacao::novos(64 * 1024, 3, 0),
        Err(ErroDeSegredo::ParametrosInvalidos)
    );
    let padrao = ParametrosDeDerivacao::default();
    assert_eq!(padrao.memoria_kib(), 64 * 1024);
    assert_eq!(padrao.iteracoes(), 3);
    assert_eq!(padrao.paralelismo(), 1);
    assert!(ParametrosDeDerivacao::novos(19 * 1024, 2, 1).is_ok());
}

/// No Unix, o diretório da guarda é só do usuário (0700) e os arquivos (cifrado e índice)
/// também (0600), inclusive depois de regravar; nenhum temporário fica para trás.
#[cfg(unix)]
#[test]
fn diretorio_e_arquivos_sao_privados_do_usuario() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, _chaveiro, guarda) = guarda_em_arquivo("permissoes");
    guarda
        .gravar(ROTULO, segredo("OUTRO_VALOR_FICTICIO"))
        .expect("regrava");
    let modo =
        |p: &std::path::Path| fs::metadata(p).expect("metadados").permissions().mode() & 0o777;
    assert_eq!(modo(dir.caminho()), 0o700, "diretório");
    assert_eq!(modo(&dir.arquivo()), 0o600, "arquivo cifrado");
    assert_eq!(modo(&dir.indice()), 0o600, "índice");
    assert!(temporarios(dir.caminho()).is_empty());
}

/// Uma gravação que falha (aqui, diretório sem permissão de escrita) não deixa arquivo parcial
/// nem temporário, não muda o que está no disco e não muda o que está na memória.
#[cfg(unix)]
#[test]
fn gravacao_que_falha_nao_deixa_arquivo_parcial_nem_muda_o_estado() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, chaveiro, guarda) = guarda_em_arquivo("falha-de-gravacao");
    let antes = fs::read(dir.arquivo()).expect("arquivo");
    fs::set_permissions(dir.caminho(), fs::Permissions::from_mode(0o500)).expect("só leitura");

    let resultado = guarda.gravar("cpf", segredo("CPF_FICTICIO_111"));
    let apagar = guarda.apagar(ROTULO);
    fs::set_permissions(dir.caminho(), fs::Permissions::from_mode(0o700)).expect("restaura");

    assert!(
        matches!(resultado, Err(ErroDeSegredo::Disco { .. })),
        "{resultado:?}"
    );
    assert!(
        matches!(apagar, Err(ErroDeSegredo::Disco { .. })),
        "{apagar:?}"
    );
    assert_eq!(
        fs::read(dir.arquivo()).expect("arquivo"),
        antes,
        "o disco não mudou"
    );
    assert!(temporarios(dir.caminho()).is_empty());
    assert_eq!(guarda.ler("cpf").unwrap_err(), ErroDeSegredo::NaoEncontrado);
    assert_eq!(guarda.ler(ROTULO).expect("ficou").expor(), VALOR);
    assert_eq!(guarda.rotulos(), vec![ROTULO.to_owned()]);

    let reaberta = abrir(dir.caminho(), &chaveiro);
    reaberta.destrancar(&segredo(SENHA)).expect("íntegro");
    assert_eq!(reaberta.rotulos(), vec![ROTULO.to_owned()]);
}

/// Quem lê o arquivo enquanto ele é regravado vê sempre uma versão inteira (a antiga ou a
/// nova), nunca um arquivo pela metade: a gravação é temporário + renomear.
#[test]
fn quem_le_durante_a_gravacao_nunca_ve_arquivo_parcial() {
    let (dir, chaveiro, guarda) = guarda_em_arquivo("concorrencia");
    let caminho = dir.arquivo();
    let escritora = {
        let guarda = guarda.clone();
        std::thread::spawn(move || {
            for i in 0..60 {
                let valor = format!("VALOR_FICTICIO_{i:04}_{}", "x".repeat(i * 37));
                guarda
                    .gravar(&format!("rotulo-{i}"), segredo(&valor))
                    .expect("grava");
            }
        })
    };
    let copia = DirTemporario::novo("concorrencia-copia");
    fs::create_dir_all(copia.caminho()).expect("cópia");
    let mut lidas = 0;
    while !escritora.is_finished() || lidas < 5 {
        let bytes = fs::read(&caminho).expect("lê o arquivo");
        fs::write(copia.arquivo(), &bytes).expect("copia");
        let _ = fs::remove_file(copia.indice());
        let leitora = abrir(copia.caminho(), &chaveiro);
        leitora
            .destrancar(&segredo(SENHA))
            .expect("toda versão lida está inteira");
        lidas += 1;
    }
    escritora.join().expect("escritora");
    assert_eq!(guarda.rotulos().len(), 61);
    assert!(temporarios(dir.caminho()).is_empty());
}

#[test]
fn guarda_trancada_nao_e_aberta_por_outra_instancia_sem_senha() {
    let (dir, chaveiro, _guarda) = guarda_em_arquivo("instancias");
    let outra = GuardaDeSegredos::abrir(configuracao(dir.caminho(), &chaveiro)).expect("abre");
    assert_eq!(outra.situacao(), Situacao::ArquivoTrancado);
    assert_eq!(outra.ler(ROTULO).unwrap_err(), ErroDeSegredo::Trancado);
}

#[test]
fn nome_do_arquivo_cifrado_configuravel() {
    let dir = DirTemporario::novo("nome");
    let chaveiro = Arc::new(ChaveiroEmMemoria::indisponivel());
    let guarda = GuardaDeSegredos::abrir(
        configuracao(dir.caminho(), &chaveiro).arquivo_cifrado("own.vault"),
    )
    .expect("abre");
    guarda.criar_arquivo(&segredo(SENHA)).expect("cria");
    assert!(dir.caminho().join("own.vault").exists());
    assert!(!dir.arquivo().exists());
}
