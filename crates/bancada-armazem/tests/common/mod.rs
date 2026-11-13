#![allow(dead_code)]
//! Apoio dos testes: diretório temporário único, apagado no `Drop`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct PastaTemporaria {
    pasta: PathBuf,
}

impl PastaTemporaria {
    pub fn nova(etiqueta: &str) -> Self {
        // Contador atômico: relógios de baixa resolução repetem o instante e dois testes
        // paralelos receberiam a mesma pasta.
        static PROXIMO: AtomicUsize = AtomicUsize::new(0);
        let n = PROXIMO.fetch_add(1, Ordering::Relaxed);
        let pasta = std::env::temp_dir().join(format!(
            "bancada-armazem-{etiqueta}-{}-{n}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("relógio depois de 1970")
                .as_nanos()
        ));
        std::fs::create_dir_all(&pasta).expect("criar pasta temporária");
        Self { pasta }
    }

    /// Caminho do banco principal dos testes.
    pub fn banco(&self) -> PathBuf {
        self.pasta.join("banco.sqlite")
    }

    pub fn pasta(&self) -> &Path {
        &self.pasta
    }
}

impl Drop for PastaTemporaria {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.pasta);
    }
}
