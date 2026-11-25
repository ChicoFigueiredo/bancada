//! O contrato entre o roteiro escrito e o app: um método por verbo da tabela `navegador`
//! (mais `util`). O trait não sabe de CDP: o app o implementa por cima do navegador, da lista
//! de bloqueio, do chaveiro e do e-mail.
//!
//! Regra dura: nenhum método recebe nem devolve segredo. Os verbos de segredo recebem só o
//! RÓTULO do chaveiro (ou nada, no caso do CPF); quem digita no campo é o motor, depois de
//! conferir o host da página atual.

use std::time::Duration;

/// Falha de um verbo. A mensagem nunca deve carregar segredo (senha, CPF, código de MFA):
/// quem implementa o trait é responsável por isso, e o host não acrescenta nada além do nome
/// do verbo.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ErroDeVerbo {
    /// A lista de bloqueio do app recusou a ação (pagar, transferir, contratar...). Encerra o
    /// roteiro; o script não consegue capturar este erro com `pcall`.
    #[error("ação bloqueada: {motivo}")]
    Bloqueado {
        /// Motivo dado pela lista de bloqueio.
        motivo: String,
    },
    /// A página (ou a URL pedida) está fora dos `hosts` do manifesto. Encerra o roteiro.
    #[error("host não permitido pelo manifesto")]
    HostNaoPermitido,
    /// O usuário abortou a pausa. Encerra o roteiro.
    #[error("o usuário abortou o roteiro")]
    Abortado,
    /// Algo não ficou pronto a tempo (download, código do e-mail...). O script pode capturar
    /// com `pcall`; se não capturar, o roteiro termina com este erro.
    #[error("prazo esgotado: {0}")]
    Prazo(String),
    /// Falha comum de execução (seletor não achado, navegador caiu...). O script pode
    /// capturar com `pcall`.
    #[error("{0}")]
    Falhou(String),
}

impl ErroDeVerbo {
    /// O mesmo erro com o texto passado por `f` (a limpeza de segredo).
    pub(crate) fn mapear(self, f: impl Fn(&str) -> String) -> Self {
        match self {
            Self::Bloqueado { motivo } => Self::Bloqueado { motivo: f(&motivo) },
            Self::Prazo(m) => Self::Prazo(f(&m)),
            Self::Falhou(m) => Self::Falhou(f(&m)),
            Self::HostNaoPermitido => Self::HostNaoPermitido,
            Self::Abortado => Self::Abortado,
        }
    }

    /// Erros que encerram o roteiro de vez, mesmo que o script tente capturá-los.
    pub(crate) fn e_fatal(&self) -> bool {
        matches!(
            self,
            Self::Bloqueado { .. } | Self::HostNaoPermitido | Self::Abortado
        )
    }
}

/// Resultado de um verbo.
pub type ResultadoDeVerbo<T> = Result<T, ErroDeVerbo>;

/// Os verbos que o roteiro escrito pode chamar. O host (`luau::executar`) valida os tipos dos
/// argumentos, aplica os tetos (espera, janela do e-mail, número de chamadas) e só então chama
/// o método; os textos que voltam já vêm limpos pelo app e são repassados ao script como estão.
///
/// # Limpeza de segredo é responsabilidade do app
///
/// O host não sabe o que é segredo. O app DEVE limpar o que guarda em [`Verbos::registrar`] e
/// implementar [`Verbos::limpar`]: o host o aplica a toda mensagem de erro e a todo registro
/// antes de devolvê-los em [`ErroDeRoteiro`](super::ErroDeRoteiro) e
/// [`Saida`](super::Saida). Sem essa implementação, nada é limpo.
pub trait Verbos {
    /// `navegador.abrir(url)`: a URL é conferida na lista de bloqueio e no host permitido.
    fn abrir(&mut self, url: &str) -> ResultadoDeVerbo<()>;

    /// `navegador.esperar(seletor, segundos?)`: `true` se o elemento ficou visível dentro do
    /// prazo. O prazo já chega limitado (padrão 15 s, máximo 120 s).
    fn esperar(&mut self, seletor: &str, prazo: Duration) -> ResultadoDeVerbo<bool>;

    /// `navegador.existe(seletor)`: visível agora?
    fn existe(&mut self, seletor: &str) -> ResultadoDeVerbo<bool>;

    /// `navegador.esperar_sumir(seletor, segundos?)`: `true` se o elemento deixou de estar
    /// visível dentro do prazo (já invisível ou removido conta como sumido); `false` se o prazo
    /// acabou com ele ainda na tela. O prazo já chega limitado (padrão 15 s, máximo 120 s). A
    /// implementação padrão consulta [`Verbos::existe`] de 250 em 250 ms; o app pode
    /// sobrescrevê-la (por exemplo, para checar cancelamento entre as consultas).
    fn esperar_sumir(&mut self, seletor: &str, prazo: Duration) -> ResultadoDeVerbo<bool> {
        let inicio = std::time::Instant::now();
        loop {
            if !self.existe(seletor)? {
                return Ok(true);
            }
            if inicio.elapsed() >= prazo {
                return Ok(false);
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    }

    /// `navegador.clicar(seletor)`: a lista de bloqueio vale aqui e no navegador.
    fn clicar(&mut self, seletor: &str) -> ResultadoDeVerbo<()>;

    /// `navegador.texto(seletor)`: texto visível do elemento, nunca valor de campo.
    fn texto(&mut self, seletor: &str) -> ResultadoDeVerbo<String>;

    /// `navegador.tabela(seletor)`: linhas e células, como texto.
    fn tabela(&mut self, seletor: &str) -> ResultadoDeVerbo<Vec<Vec<String>>>;

    /// `navegador.preencher(seletor, texto)`: recusa campo de senha.
    fn preencher(&mut self, seletor: &str, texto: &str) -> ResultadoDeVerbo<()>;

    /// `navegador.preencher_senha(seletor, rotulo)`: o app busca o segredo no chaveiro pelo
    /// rótulo e o motor digita direto no campo, só se a página é de host aprovado.
    fn preencher_senha(&mut self, seletor: &str, rotulo: &str) -> ResultadoDeVerbo<()>;

    /// `navegador.preencher_cpf(seletor)`: o CPF vem do chaveiro, pelo rótulo do manifesto.
    fn preencher_cpf(&mut self, seletor: &str) -> ResultadoDeVerbo<()>;

    /// `navegador.preencher_codigo_do_email(seletor, opcoes?)`: o remetente e o formato do
    /// código vêm do manifesto. A janela já chega limitada (padrão 120 s, máximo 300 s).
    fn preencher_codigo_do_email(
        &mut self,
        seletor: &str,
        janela: Duration,
    ) -> ResultadoDeVerbo<()>;

    /// `navegador.baixar(seletor, nome?)`: devolve só o nome do arquivo, nunca o conteúdo.
    /// O `nome`, se houver, já foi conferido pelo host (sem separador de pasta).
    fn baixar(&mut self, seletor: &str, nome: Option<&str>) -> ResultadoDeVerbo<String>;

    /// `navegador.pausa(motivo)`: passa a vez ao usuário e só retorna quando ele continua;
    /// `Err(Abortado)` se ele aborta. O tempo gasto aqui não conta no tempo total do roteiro.
    fn pausa(&mut self, motivo: &str) -> ResultadoDeVerbo<()>;

    /// `navegador.registrar(mensagem)`: o app limpa e guarda o registro. O host também passa o
    /// registro por [`Verbos::limpar`] antes de pô-lo em [`Saida`](super::Saida), mas o que o
    /// app guarda por conta própria aqui é da responsabilidade dele.
    fn registrar(&mut self, mensagem: &str) -> ResultadoDeVerbo<()>;

    /// Remove segredo (senha, CPF, código de MFA) de um texto. O host o aplica a toda mensagem
    /// de erro (do verbo e do roteiro) e a todo registro antes de devolvê-los ao app e ao
    /// script. A implementação padrão devolve o texto como está: o app DEVE sobrescrevê-la.
    fn limpar(&self, texto: &str) -> String {
        texto.to_owned()
    }

    /// Tempo que um verbo gastou esperando o usuário por conta própria, por exemplo uma pausa
    /// automática por CAPTCHA ou código do aplicativo detectada dentro de `abrir`, `clicar` ou
    /// `esperar`. O host chama este método após CADA verbo, soma o que vier ao desconto do
    /// tempo total e o app deve zerar o acumulado a cada chamada. Não inclua aqui o tempo de
    /// `navegador.pausa`: o host já o mede. A implementação padrão não desconta nada.
    fn tempo_pausado_automatico(&mut self) -> Duration {
        Duration::ZERO
    }

    /// `util.hoje()`: "AAAA-MM-DD" no fuso do usuário (UTC−3), nunca UTC puro.
    fn hoje(&mut self) -> ResultadoDeVerbo<String>;

    /// `util.somar_meses(data, n)`: "AAAA-MM-DD" (`n` pode ser negativo).
    fn somar_meses(&mut self, data: &str, meses: i32) -> ResultadoDeVerbo<String>;
}
