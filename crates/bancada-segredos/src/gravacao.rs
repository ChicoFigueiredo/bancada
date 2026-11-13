//! Disco: diretório privado e gravação atômica.

use crate::erro::ErroDeSegredo;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

/// Cria `dir` (e os pais) legível só pelo usuário no Unix (0700). Um diretório que já existe
/// não tem a permissão alterada.
pub(crate) fn criar_diretorio_privado(dir: &Path) -> Result<(), ErroDeSegredo> {
    let mut construtor = fs::DirBuilder::new();
    construtor.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        construtor.mode(0o700);
    }
    construtor.create(dir).map_err(|e| ErroDeSegredo::disco(&e))
}

/// Grava `bytes` em `caminho` de forma atômica: um temporário novo no mesmo diretório
/// (`create_new`, nunca segue um link que já esteja lá), legível só pelo usuário no Unix
/// (0600), `fsync`, e então renomeia por cima do destino. Quem lê o destino vê a versão
/// antiga inteira ou a nova inteira. Se algo falha, o temporário é apagado e o destino fica
/// como estava.
pub(crate) fn gravar_atomicamente(caminho: &Path, bytes: &[u8]) -> Result<(), ErroDeSegredo> {
    static PROXIMO: AtomicU64 = AtomicU64::new(0);
    let nome = caminho
        .file_name()
        .ok_or(ErroDeSegredo::Interno {
            detalhe: "caminho sem nome de arquivo",
        })?
        .to_string_lossy();
    let dir = caminho.parent().unwrap_or_else(|| Path::new("."));
    let temporario = dir.join(format!(
        ".{nome}.{}.{}.tmp",
        std::process::id(),
        PROXIMO.fetch_add(1, Ordering::Relaxed)
    ));

    let mut opcoes = fs::OpenOptions::new();
    opcoes.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opcoes.mode(0o600);
    }
    let mut arquivo = opcoes
        .open(&temporario)
        .map_err(|e| ErroDeSegredo::disco(&e))?;
    let escrito = arquivo.write_all(bytes).and_then(|()| arquivo.sync_all());
    drop(arquivo);
    if let Err(e) = escrito.and_then(|()| fs::rename(&temporario, caminho)) {
        let _ = fs::remove_file(&temporario);
        return Err(ErroDeSegredo::disco(&e));
    }
    // A entrada nova do diretório também vai para o disco (sem isso, uma queda logo depois
    // pode voltar ao arquivo antigo). Sistemas que não sincronizam diretório são ignorados.
    #[cfg(unix)]
    if let Ok(d) = fs::File::open(dir) {
        let _ = d.sync_all();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(nome: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "bancada-segredos-gravacao-{nome}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn sobras(d: &Path) -> Vec<String> {
        fs::read_dir(d)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".tmp"))
            .collect()
    }

    #[test]
    fn substitui_o_destino_inteiro() {
        let d = dir("substitui");
        let alvo = d.join("alvo");
        gravar_atomicamente(&alvo, b"primeira versao, mais longa").unwrap();
        gravar_atomicamente(&alvo, b"segunda").unwrap();
        assert_eq!(fs::read(&alvo).unwrap(), b"segunda");
        assert!(sobras(&d).is_empty());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn renomear_que_falha_nao_deixa_temporario_nem_mexe_no_destino() {
        let d = dir("renomear-falha");
        // Destino é um diretório com conteúdo: o `rename` de um arquivo por cima falha.
        let alvo = d.join("alvo");
        fs::create_dir_all(alvo.join("dentro")).unwrap();
        let erro = gravar_atomicamente(&alvo, b"conteudo").unwrap_err();
        assert!(matches!(erro, ErroDeSegredo::Disco { .. }), "{erro:?}");
        assert!(alvo.join("dentro").is_dir(), "destino intacto");
        assert!(sobras(&d).is_empty(), "{:?}", sobras(&d));
        let _ = fs::remove_dir_all(&d);
    }

    #[cfg(unix)]
    #[test]
    fn nao_segue_link_simbolico_plantado_no_lugar_do_temporario() {
        // O nome do temporário é único por gravação; mesmo um link com o nome exato não seria
        // seguido, porque `create_new` falha em vez de abrir o alvo do link.
        let d = dir("link");
        let fora = d.join("fora");
        fs::write(&fora, b"intocado").unwrap();
        let plantado = d.join("plantado.tmp");
        std::os::unix::fs::symlink(&fora, &plantado).unwrap();
        let mut opcoes = fs::OpenOptions::new();
        opcoes.write(true).create_new(true);
        assert!(opcoes.open(&plantado).is_err());
        assert_eq!(fs::read(&fora).unwrap(), b"intocado");
        let _ = fs::remove_dir_all(&d);
    }
}
