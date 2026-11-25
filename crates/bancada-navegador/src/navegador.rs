//! O contrato do navegador. Quem o usa (o executor do app, o host Luau) só conhece este
//! trait; o Chrome por CDP ([`crate::chrome::Chrome`]) e o navegador falso dos testes o implementam.

use crate::erro::Resultado;
use crate::pagina::Pagina;
use crate::politica::{Bloqueio, Candidato};
use bancada_segredos::Segredo;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Que tipo de campo deve receber o segredo: o motor confere isto de novo, dentro do
/// navegador, imediatamente antes de digitar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipoDeCampo {
    /// `input type=password` (a senha do chaveiro).
    Senha,
    /// Campo de texto (`text`, `tel`, `number`, `search`): o CPF e o código de verificação.
    Texto,
}

/// Um navegador controlável. Nenhum método devolve valor de campo.
pub trait Navegador {
    /// Abre a URL e espera carregar.
    fn navegar(&mut self, url: &str) -> Resultado<()>;
    /// O seletor existe e está visível agora?
    fn visivel(&mut self, seletor: &str) -> Resultado<bool>;
    /// Descreve o elemento para a lista de bloqueio; `None` se não existe.
    fn descrever(&mut self, seletor: &str) -> Resultado<Option<Candidato>>;
    /// Clica (clique de verdade no centro do elemento).
    fn clicar(&mut self, seletor: &str) -> Resultado<()>;
    /// Preenche com texto fixo.
    fn preencher(&mut self, seletor: &str, texto: &str) -> Resultado<()>;
    /// Preenche com um segredo (senha, CPF, código). O segredo só existe em memória, entre o
    /// chaveiro e o campo; a implementação não o copia para log, erro ou `Debug`.
    ///
    /// **TOCTOU.** Quem chama confere a página antes (host aprovado, campo certo), mas entre essa
    /// conferência e a digitação a página pode ter navegado (um redirecionamento por timer). Por
    /// isso a decisão final é do motor: imediatamente antes de digitar, a implementação reconfere
    /// que (1) a página não navegou desde a última leitura de [`Navegador::pagina`]; (2) a URL que
    /// o **navegador** carregou (nunca a que o JavaScript da página informa) passa em
    /// `url_permitida`; (3) o elemento ainda é o que tem o foco e é do `tipo` esperado. Se algo
    /// mudou, **aborta sem digitar** (a mensagem do erro não leva o segredo).
    ///
    /// `url_permitida` recebe a URL completa da página (o app decide esquema, host, credenciais
    /// embutidas e, nos testes, o mapa de hosts).
    ///
    /// # Errors
    /// A página mudou, o campo não é o esperado, ou falha do navegador.
    fn preencher_segredo(
        &mut self,
        seletor: &str,
        segredo: &Segredo,
        tipo: TipoDeCampo,
        url_permitida: &dyn Fn(&str) -> bool,
    ) -> Resultado<()>;
    /// Clica e espera o arquivo baixado cair em `pasta`.
    fn clicar_e_baixar(
        &mut self,
        seletor: &str,
        pasta: &Path,
        prazo: Duration,
    ) -> Resultado<PathBuf>;
    /// O texto **visível** do elemento (o que o usuário lê). Nunca o valor de um `<input>`,
    /// `<textarea>` ou campo de senha: para esses, é erro. O Chrome e o navegador falso o
    /// implementam; quem não implementa (um navegador de teste que não lê texto) recusa.
    fn texto(&mut self, _seletor: &str) -> Resultado<String> {
        Err(crate::erro::ErroDoNavegador::Navegador(
            "este navegador não lê texto".into(),
        ))
    }
    /// As linhas da tabela, como texto.
    fn ler_tabela(&mut self, seletor: &str) -> Resultado<Vec<Vec<String>>>;
    /// A página simplificada (sem valores de campo).
    fn pagina(&mut self) -> Resultado<Pagina>;
    /// Os bloqueios que o script do navegador barrou desde a última chamada.
    fn bloqueios(&mut self) -> Resultado<Vec<Bloqueio>>;

    /// Marca a fase de login: enquanto `host` é `Some`, o navegador só deixa sair requisição que
    /// muda estado (`POST` e afins) para esse host (RO-02). `None` encerra a fase. Opcional.
    fn fase_de_login(&mut self, _host: Option<&str>) {}

    /// Traz a janela do navegador para a frente (a pausa "Sua vez no navegador"). Opcional.
    fn trazer_para_frente(&mut self) {}
}
