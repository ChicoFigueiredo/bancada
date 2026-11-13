//! Apoio dos testes da guarda: diretório temporário, chaveiro do sistema falso, custo baixo do
//! Argon2id e a varredura do disco atrás de segredo em claro. Só valores fictícios.
#![allow(dead_code)]

use bancada_segredos::testes::ChaveiroEmMemoria;
use bancada_segredos::{ConfiguracaoDaGuarda, GuardaDeSegredos, ParametrosDeDerivacao, Segredo};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

pub const SERVICO: &str = "Servico De Teste";
pub const ROTULO: &str = "senha:banco-ficticio";
pub const VALOR: &str = "VALOR_FICTICIO_DE_TESTE_7f3a9c";
pub const SENHA: &str = "senha-mestra-ficticia-longa";
pub const ARQUIVO: &str = "segredos.cifrado";
pub const INDICE: &str = "rotulos.json";

/// Diretório temporário novo, apagado ao descartar.
#[derive(Debug)]
pub struct DirTemporario(PathBuf);

impl DirTemporario {
    pub fn novo(nome: &str) -> Self {
        static PROXIMO: AtomicUsize = AtomicUsize::new(0);
        let n = PROXIMO.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "bancada-segredos-{nome}-{}-{n}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        Self(dir)
    }

    pub fn caminho(&self) -> &Path {
        &self.0
    }

    pub fn arquivo(&self) -> PathBuf {
        self.0.join(ARQUIVO)
    }

    pub fn indice(&self) -> PathBuf {
        self.0.join(INDICE)
    }
}

impl Drop for DirTemporario {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub fn segredo(texto: &str) -> Segredo {
    Segredo::novo(texto)
}

pub fn configuracao(dir: &Path, chaveiro: &Arc<ChaveiroEmMemoria>) -> ConfiguracaoDaGuarda {
    ConfiguracaoDaGuarda::nova(SERVICO, dir)
        .chaveiro(chaveiro.clone())
        .derivacao(ParametrosDeDerivacao::rapidos_para_testes())
}

pub fn abrir(dir: &Path, chaveiro: &Arc<ChaveiroEmMemoria>) -> GuardaDeSegredos {
    GuardaDeSegredos::abrir(configuracao(dir, chaveiro)).expect("abre a guarda")
}

/// Guarda com o arquivo cifrado criado (sem chaveiro do sistema) e [`VALOR`] em [`ROTULO`].
pub fn guarda_em_arquivo(nome: &str) -> (DirTemporario, Arc<ChaveiroEmMemoria>, GuardaDeSegredos) {
    let dir = DirTemporario::novo(nome);
    let chaveiro = Arc::new(ChaveiroEmMemoria::indisponivel());
    let guarda = abrir(dir.caminho(), &chaveiro);
    guarda
        .criar_arquivo(&segredo(SENHA))
        .expect("cria o arquivo cifrado");
    guarda.gravar(ROTULO, segredo(VALOR)).expect("grava");
    (dir, chaveiro, guarda)
}

/// Guarda sobre o chaveiro do sistema (falso).
pub fn guarda_no_chaveiro(nome: &str) -> (DirTemporario, Arc<ChaveiroEmMemoria>, GuardaDeSegredos) {
    let dir = DirTemporario::novo(nome);
    let chaveiro = Arc::new(ChaveiroEmMemoria::novo());
    let guarda = abrir(dir.caminho(), &chaveiro);
    (dir, chaveiro, guarda)
}

/// Todos os arquivos sob `dir` (recursivo), em ordem.
pub fn arquivos(dir: &Path) -> Vec<PathBuf> {
    let mut saida = Vec::new();
    let Ok(entradas) = fs::read_dir(dir) else {
        return saida;
    };
    for entrada in entradas.flatten() {
        let caminho = entrada.path();
        if caminho.is_dir() {
            saida.extend(arquivos(&caminho));
        } else {
            saida.push(caminho);
        }
    }
    saida.sort();
    saida
}

pub fn contem(bytes: &[u8], agulha: &[u8]) -> bool {
    !agulha.is_empty() && bytes.windows(agulha.len()).any(|janela| janela == agulha)
}

/// Arquivos sob `dir` que contêm `agulha` em claro.
pub fn arquivos_contendo(dir: &Path, agulha: &str) -> Vec<PathBuf> {
    arquivos(dir)
        .into_iter()
        .filter(|p| fs::read(p).is_ok_and(|bytes| contem(&bytes, agulha.as_bytes())))
        .collect()
}

/// Arquivos temporários de gravação que ficaram para trás.
pub fn temporarios(dir: &Path) -> Vec<PathBuf> {
    arquivos(dir)
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "tmp"))
        .collect()
}
