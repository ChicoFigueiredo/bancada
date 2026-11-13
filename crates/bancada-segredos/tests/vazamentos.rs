//! O que não pode vazar: o valor de teste nunca aparece em `Debug`, em mensagem de erro nem nos
//! bytes do disco; senha errada e arquivo adulterado (cabeçalho, nonce, corpo) dão o mesmo
//! erro, com a mesma mensagem; arquivo cortado em qualquer ponto é recusado sem pânico.

mod comum;

use bancada_segredos::testes::ChaveiroEmMemoria;
use bancada_segredos::{ErroDeSegredo, FalhaDoChaveiro, Situacao};
use comum::*;
use std::fs;
use std::path::Path;
use std::sync::Arc;

const TAMANHO_DO_CABECALHO: usize = 96;

#[test]
fn debug_do_segredo_nao_mostra_o_valor() {
    let valor = segredo(VALOR);
    assert_eq!(format!("{valor:?}"), "Segredo(***)");
    assert_eq!(format!("{valor:#?}"), "Segredo(***)");
    assert_eq!(format!("{:?}", Some(valor.clone())), "Some(Segredo(***))");
}

#[test]
fn nada_de_valor_ou_senha_em_debug_nem_em_erro() {
    let (_dir, _chaveiro, guarda) = guarda_em_arquivo("debug");
    let config = configuracao(
        Path::new("/nao/existe"),
        &Arc::new(ChaveiroEmMemoria::novo()),
    );
    let erros = [
        ErroDeSegredo::Indisponivel,
        ErroDeSegredo::Trancado,
        ErroDeSegredo::SemArquivoCifrado,
        ErroDeSegredo::NaoPodeCriarArquivo,
        ErroDeSegredo::SenhaOuArquivoInvalido,
        ErroDeSegredo::FormatoNaoSuportado { versao: 9 },
        ErroDeSegredo::SenhaFraca { minimo: 8 },
        ErroDeSegredo::ParametrosInvalidos,
        ErroDeSegredo::SegredoVazio,
        ErroDeSegredo::RotuloInvalido,
        ErroDeSegredo::NaoEncontrado,
        ErroDeSegredo::IndiceCorrompido,
        ErroDeSegredo::ChaveiroDoSistema(FalhaDoChaveiro::AcessoNegado),
        ErroDeSegredo::Disco {
            tipo: std::io::ErrorKind::PermissionDenied,
        },
        ErroDeSegredo::Interno { detalhe: "teste" },
    ];
    let mut textos = vec![
        format!("{guarda:?}"),
        format!("{:?}", guarda.ler(ROTULO)),
        format!("{:?}", guarda.rotulos()),
        format!("{config:?}"),
        format!("{:?}", guarda.destrancar(&segredo("senha-errada-123"))),
        format!(
            "{:?}",
            guarda.trocar_senha_mestra(&segredo(SENHA), &segredo("curta"))
        ),
    ];
    for erro in &erros {
        textos.push(format!("{erro:?}"));
        textos.push(erro.to_string());
        assert!(!erro.to_string().is_empty());
    }
    for texto in textos {
        for proibido in [VALOR, SENHA, "FICTICIO"] {
            assert!(!texto.contains(proibido), "vazou {proibido:?} em: {texto}");
        }
    }
}

#[test]
fn o_valor_e_a_senha_nunca_estao_nos_bytes_do_disco() {
    let (dir, _chaveiro, guarda) = guarda_em_arquivo("bytes");
    guarda
        .gravar("cpf", segredo("CPF_FICTICIO_123"))
        .expect("grava");
    guarda
        .trocar_senha_mestra(&segredo(SENHA), &segredo("nova-senha-ficticia-xyz"))
        .expect("troca");
    let cifrado = fs::read(dir.arquivo()).expect("arquivo cifrado");
    for agulha in [
        VALOR,
        &VALOR[..12],
        "CPF_FICTICIO_123",
        SENHA,
        "nova-senha-ficticia-xyz",
        ROTULO,
        "cpf",
    ] {
        assert!(
            !contem(&cifrado, agulha.as_bytes()),
            "{agulha:?} em claro no arquivo cifrado"
        );
    }
    for agulha in [VALOR, "CPF_FICTICIO_123", SENHA, "nova-senha-ficticia-xyz"] {
        assert!(
            arquivos_contendo(dir.caminho(), agulha).is_empty(),
            "{agulha:?} em claro no disco"
        );
    }
}

/// Cada caso: o que foi feito e os bytes resultantes.
fn adulterados(original: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut casos = Vec::new();
    // Um byte trocado em cada posição do cabeçalho (assinatura, KDF, parâmetros, sal,
    // verificador, cifra e nonce), menos a versão, que tem teste próprio.
    for posicao in (0..TAMANHO_DO_CABECALHO).filter(|p| !(8..10).contains(p)) {
        let mut bytes = original.to_vec();
        bytes[posicao] ^= 0x01;
        casos.push((format!("cabeçalho, byte {posicao}"), bytes));
    }
    // No corpo: o primeiro byte cifrado, um do meio, o primeiro e o último da etiqueta.
    let tamanho = original.len();
    for posicao in [
        TAMANHO_DO_CABECALHO,
        (TAMANHO_DO_CABECALHO + tamanho) / 2,
        tamanho - 16,
        tamanho - 1,
    ] {
        let mut bytes = original.to_vec();
        bytes[posicao] ^= 0x80;
        casos.push((format!("corpo, byte {posicao}"), bytes));
    }
    let mut acrescido = original.to_vec();
    acrescido.push(0);
    casos.push(("byte a mais no fim".to_owned(), acrescido));
    let mut versao_zero = original.to_vec();
    versao_zero[8..10].copy_from_slice(&0u16.to_le_bytes());
    casos.push(("versão 0".to_owned(), versao_zero));
    casos
}

fn confere_mesmo_erro(dir: &Path, arquivo: &Path, chaveiro: &Arc<ChaveiroEmMemoria>, senha: &str) {
    let original = fs::read(arquivo).expect("arquivo");
    let guarda = abrir(dir, chaveiro);
    let senha_errada = guarda
        .destrancar(&segredo("senha-errada-123"))
        .expect_err("senha errada");
    assert_eq!(senha_errada, ErroDeSegredo::SenhaOuArquivoInvalido);
    let mensagem = senha_errada.to_string();

    for (o_que, bytes) in adulterados(&original) {
        fs::write(arquivo, &bytes).expect("grava o adulterado");
        let guarda = abrir(dir, chaveiro);
        let erro = guarda.destrancar(&segredo(senha)).expect_err("adulterado");
        assert_eq!(erro, senha_errada, "{o_que}");
        assert_eq!(erro.to_string(), mensagem, "{o_que}");
        assert_eq!(guarda.situacao(), Situacao::ArquivoTrancado, "{o_que}");
        assert!(guarda.ler(ROTULO).is_err(), "{o_que}: nada lido");
    }

    fs::write(arquivo, &original).expect("restaura");
    abrir(dir, chaveiro)
        .destrancar(&segredo(senha))
        .expect("íntegro de novo");
}

#[test]
fn senha_errada_e_arquivo_adulterado_dao_o_mesmo_erro() {
    let (dir, chaveiro, guarda) = guarda_em_arquivo("adulterado");
    drop(guarda);
    confere_mesmo_erro(dir.caminho(), &dir.arquivo(), &chaveiro, SENHA);
}

#[test]
fn o_mesmo_vale_para_o_arquivo_do_chess_vault() {
    let dir = DirTemporario::novo("adulterado-v1");
    fs::create_dir_all(dir.caminho()).expect("diretório");
    fs::copy("tests/fixtures/chess-vault-v1.vault", dir.arquivo()).expect("copia a fixture");
    let chaveiro = Arc::new(ChaveiroEmMemoria::indisponivel());
    confere_mesmo_erro(
        dir.caminho(),
        &dir.arquivo(),
        &chaveiro,
        "senha-mestra-ficticia",
    );
}

#[test]
fn versao_mais_nova_e_formato_nao_suportado() {
    let (dir, chaveiro, guarda) = guarda_em_arquivo("versao-nova");
    drop(guarda);
    let mut bytes = fs::read(dir.arquivo()).expect("arquivo");
    bytes[8..10].copy_from_slice(&99u16.to_le_bytes());
    fs::write(dir.arquivo(), &bytes).expect("grava");
    let guarda = abrir(dir.caminho(), &chaveiro);
    assert_eq!(
        guarda.destrancar(&segredo(SENHA)),
        Err(ErroDeSegredo::FormatoNaoSuportado { versao: 99 })
    );
}

#[test]
fn arquivo_cortado_em_qualquer_ponto_e_recusado_sem_panico() {
    let (dir, chaveiro, guarda) = guarda_em_arquivo("cortado");
    drop(guarda);
    let original = fs::read(dir.arquivo()).expect("arquivo");
    for tamanho in 0..original.len() {
        fs::write(dir.arquivo(), &original[..tamanho]).expect("grava o cortado");
        let guarda = abrir(dir.caminho(), &chaveiro);
        assert_eq!(
            guarda.destrancar(&segredo(SENHA)),
            Err(ErroDeSegredo::SenhaOuArquivoInvalido),
            "cortado em {tamanho} bytes"
        );
        assert!(guarda.ler(ROTULO).is_err());
    }
}
