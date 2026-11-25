//! A ponte entre o Luau e o trait [`Verbos`]: cria as tabelas `navegador` e `util`, valida os
//! tipos dos argumentos, aplica os tetos e conta as chamadas.

use std::cell::{Cell, RefCell};
use std::sync::Arc;
use std::time::{Duration, Instant};

use mlua::{Error, IntoLuaMulti, Lua, Result, Scope, Table, Value, Variadic};

use super::limites::{Controle, ErroDeRoteiro, Saida};
use super::verbos::{ResultadoDeVerbo, Verbos};

/// Espera padrão de `navegador.esperar`.
pub const ESPERA_PADRAO: Duration = Duration::from_secs(15);
/// Teto de `navegador.esperar`.
pub const ESPERA_MAXIMA: Duration = Duration::from_secs(120);
/// Janela padrão de `navegador.preencher_codigo_do_email`.
pub const JANELA_PADRAO: Duration = Duration::from_secs(120);
/// Teto da janela de `navegador.preencher_codigo_do_email`.
pub const JANELA_MAXIMA: Duration = Duration::from_secs(300);

/// Tamanho máximo, em caracteres, de uma mensagem de `navegador.registrar`.
const REGISTRO_MAXIMO: usize = 2_000;
/// Intervalo aceito por `util.somar_meses`.
const MESES_MAXIMOS: i64 = 1_200;

/// Tudo o que as funções Lua precisam: o trait, os tetos, a contagem e a saída.
pub(crate) struct Contexto<'a> {
    verbos: RefCell<&'a mut dyn Verbos>,
    controle: Arc<Controle>,
    chamadas: Cell<u32>,
    saida: RefCell<Saida>,
}

impl<'a> Contexto<'a> {
    pub(crate) fn novo(verbos: &'a mut dyn Verbos, controle: Arc<Controle>) -> Self {
        Self {
            verbos: RefCell::new(verbos),
            controle,
            chamadas: Cell::new(0),
            saida: RefCell::new(Saida::default()),
        }
    }

    pub(crate) fn tomar_saida(&self) -> Saida {
        self.saida.take()
    }

    /// Passa por todas as conferências (encerramento, tempo, nº de chamadas) e chama o verbo.
    fn chamar<R>(&self, f: impl FnOnce(&mut dyn Verbos) -> ResultadoDeVerbo<R>) -> Result<R> {
        if self.controle.esta_encerrado() {
            return Err(Error::runtime("roteiro encerrado"));
        }
        self.controle.conferir_tempo().map_err(Error::runtime)?;
        let n = self.chamadas.get().saturating_add(1);
        self.chamadas.set(n);
        let maximo = self.controle.limites().chamadas;
        if n > maximo {
            self.controle
                .marcar_fatal(ErroDeRoteiro::LimiteDeChamadas(maximo));
            return Err(Error::runtime("limite de chamadas a verbos"));
        }
        let mut verbos = self
            .verbos
            .try_borrow_mut()
            .map_err(|_| Error::runtime("chamada recursiva a um verbo"))?;
        let resultado = f(&mut **verbos);
        // Pausas automáticas (CAPTCHA, código do aplicativo) dentro do verbo não contam no
        // tempo total, nem quando o verbo falhou.
        self.controle.somar_pausa(verbos.tempo_pausado_automatico());
        match resultado {
            Ok(r) => Ok(r),
            Err(e) => {
                let e = e.mapear(|t| verbos.limpar(t));
                if e.e_fatal() {
                    self.controle.marcar_fatal(ErroDeRoteiro::Verbo {
                        erro: e.clone(),
                        linha: None,
                    });
                }
                Err(Error::external(e))
            }
        }
    }

    /// Passa o texto pela limpeza de segredo do app.
    pub(crate) fn limpar(&self, texto: &str) -> String {
        match self.verbos.try_borrow() {
            Ok(v) => v.limpar(texto),
            // Não deveria acontecer (o roteiro já terminou); na dúvida, não devolve o texto.
            Err(_) => "[texto omitido]".to_owned(),
        }
    }
}

// ---------------------------------------------------------------------------------------
// Validação de argumentos
// ---------------------------------------------------------------------------------------

fn tipo_de(v: &Value) -> &'static str {
    match v {
        Value::Nil => "nada (nil)",
        Value::Boolean(_) => "booleano",
        Value::Integer(_) | Value::Number(_) => "número",
        Value::String(_) => "texto",
        Value::Table(_) => "tabela",
        Value::Function(_) => "função",
        _ => "outro tipo",
    }
}

fn erro_arg(verbo: &str, pos: usize, nome: &str, esperado: &str, recebido: &Value) -> Error {
    Error::runtime(format!(
        "{verbo}: o argumento {pos} ({nome}) deve ser {esperado}, mas veio {}",
        tipo_de(recebido)
    ))
}

/// Tira os `nil` do fim (`f(a, nil)` vale como `f(a)`).
fn aparar(args: Variadic<Value>) -> Vec<Value> {
    let mut v: Vec<Value> = args.into_iter().collect();
    while matches!(v.last(), Some(Value::Nil)) {
        v.pop();
    }
    v
}

fn sem_sobras(verbo: &str, args: &[Value], maximo: usize) -> Result<()> {
    if args.len() > maximo {
        return Err(Error::runtime(format!(
            "{verbo}: recebeu {} argumentos, mas aceita no máximo {maximo}",
            args.len()
        )));
    }
    Ok(())
}

fn arg_texto(verbo: &str, args: &[Value], i: usize, nome: &str) -> Result<String> {
    match args.get(i) {
        Some(Value::String(s)) => s.to_str().map(|t| t.to_string()).map_err(|_| {
            Error::runtime(format!(
                "{verbo}: o argumento {} ({nome}) não é um texto UTF-8 válido",
                i + 1
            ))
        }),
        Some(outro) => Err(erro_arg(verbo, i + 1, nome, "um texto", outro)),
        None => Err(Error::runtime(format!(
            "{verbo}: falta o argumento {} ({nome}), que deve ser um texto",
            i + 1
        ))),
    }
}

fn arg_texto_opcional(verbo: &str, args: &[Value], i: usize, nome: &str) -> Result<Option<String>> {
    if args.get(i).is_none() {
        return Ok(None);
    }
    arg_texto(verbo, args, i, nome).map(Some)
}

fn como_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Integer(i) => Some(*i as f64),
        Value::Number(n) => Some(*n),
        _ => None,
    }
}

/// Segundos positivos e finitos, limitados a `maximo`.
fn segundos_limitados(
    verbo: &str,
    pos: usize,
    nome: &str,
    v: &Value,
    maximo: Duration,
) -> Result<Duration> {
    let Some(n) = como_f64(v) else {
        return Err(erro_arg(verbo, pos, nome, "um número de segundos", v));
    };
    if !n.is_finite() || n <= 0.0 {
        return Err(Error::runtime(format!(
            "{verbo}: o argumento {pos} ({nome}) deve ser um número de segundos maior que zero"
        )));
    }
    Ok(Duration::try_from_secs_f64(n).map_or(maximo, |d| d.min(maximo)))
}

/// `nome` de arquivo sem separador de pasta, para o script não escolher onde gravar.
fn conferir_nome_de_arquivo(verbo: &str, nome: &str) -> Result<()> {
    let ruim = nome.is_empty()
        || nome == "."
        || nome == ".."
        || nome.chars().any(|c| matches!(c, '/' | '\\' | '\0'));
    if ruim {
        return Err(Error::runtime(format!(
            "{verbo}: o nome do arquivo não pode ser vazio nem ter barra ou ponto-ponto"
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------
// Registro das tabelas
// ---------------------------------------------------------------------------------------

fn adicionar<'s, 'e, F, R>(scope: &'s Scope<'s, 'e>, tabela: &Table, nome: &str, f: F) -> Result<()>
where
    F: Fn(&[Value]) -> Result<R> + 's,
    R: IntoLuaMulti,
{
    let funcao = scope.create_function(move |_, args: Variadic<Value>| f(&aparar(args)))?;
    tabela.set(nome, funcao)
}

/// Cria as tabelas `navegador` e `util` (somente leitura) e as põe nos globais.
pub(crate) fn instalar<'s, 'e: 's>(
    lua: &Lua,
    scope: &'s Scope<'s, 'e>,
    ctx: &'e Contexto<'e>,
) -> Result<()> {
    let nav = lua.create_table()?;
    let util = lua.create_table()?;

    adicionar(scope, &nav, "abrir", move |a| {
        const V: &str = "navegador.abrir";
        sem_sobras(V, a, 1)?;
        let url = arg_texto(V, a, 0, "url")?;
        ctx.chamar(|v| v.abrir(&url))
    })?;

    adicionar(scope, &nav, "esperar", move |a| {
        const V: &str = "navegador.esperar";
        sem_sobras(V, a, 2)?;
        let seletor = arg_texto(V, a, 0, "seletor")?;
        let prazo = match a.get(1) {
            None => ESPERA_PADRAO,
            Some(v) => segundos_limitados(V, 2, "segundos", v, ESPERA_MAXIMA)?,
        };
        ctx.chamar(|v| v.esperar(&seletor, prazo))
    })?;

    adicionar(scope, &nav, "existe", move |a| {
        const V: &str = "navegador.existe";
        sem_sobras(V, a, 1)?;
        let seletor = arg_texto(V, a, 0, "seletor")?;
        ctx.chamar(|v| v.existe(&seletor))
    })?;

    adicionar(scope, &nav, "clicar", move |a| {
        const V: &str = "navegador.clicar";
        sem_sobras(V, a, 1)?;
        let seletor = arg_texto(V, a, 0, "seletor")?;
        ctx.chamar(|v| v.clicar(&seletor))
    })?;

    adicionar(scope, &nav, "texto", move |a| {
        const V: &str = "navegador.texto";
        sem_sobras(V, a, 1)?;
        let seletor = arg_texto(V, a, 0, "seletor")?;
        ctx.chamar(|v| v.texto(&seletor))
    })?;

    adicionar(scope, &nav, "tabela", move |a| {
        const V: &str = "navegador.tabela";
        sem_sobras(V, a, 1)?;
        let seletor = arg_texto(V, a, 0, "seletor")?;
        ctx.chamar(|v| v.tabela(&seletor))
    })?;

    adicionar(scope, &nav, "preencher", move |a| {
        const V: &str = "navegador.preencher";
        sem_sobras(V, a, 2)?;
        let seletor = arg_texto(V, a, 0, "seletor")?;
        let texto = arg_texto(V, a, 1, "texto")?;
        ctx.chamar(|v| v.preencher(&seletor, &texto))
    })?;

    adicionar(scope, &nav, "preencher_senha", move |a| {
        const V: &str = "navegador.preencher_senha";
        sem_sobras(V, a, 2)?;
        let seletor = arg_texto(V, a, 0, "seletor")?;
        let rotulo = arg_texto(V, a, 1, "rotulo")?;
        ctx.chamar(|v| v.preencher_senha(&seletor, &rotulo))
    })?;

    adicionar(scope, &nav, "preencher_cpf", move |a| {
        const V: &str = "navegador.preencher_cpf";
        // Sem segundo argumento de propósito: o CPF nunca passa pelo script.
        sem_sobras(V, a, 1)?;
        let seletor = arg_texto(V, a, 0, "seletor")?;
        ctx.chamar(|v| v.preencher_cpf(&seletor))
    })?;

    adicionar(scope, &nav, "preencher_codigo_do_email", move |a| {
        const V: &str = "navegador.preencher_codigo_do_email";
        sem_sobras(V, a, 2)?;
        let seletor = arg_texto(V, a, 0, "seletor")?;
        let janela = janela_do_email(V, a.get(1))?;
        ctx.chamar(|v| v.preencher_codigo_do_email(&seletor, janela))
    })?;

    adicionar(scope, &nav, "baixar", move |a| {
        const V: &str = "navegador.baixar";
        sem_sobras(V, a, 2)?;
        let seletor = arg_texto(V, a, 0, "seletor")?;
        let nome = arg_texto_opcional(V, a, 1, "nome")?;
        if let Some(n) = &nome {
            conferir_nome_de_arquivo(V, n)?;
        }
        let arquivo = ctx.chamar(|v| v.baixar(&seletor, nome.as_deref()))?;
        ctx.saida.borrow_mut().arquivos.push(arquivo.clone());
        Ok(arquivo)
    })?;

    adicionar(scope, &nav, "pausa", move |a| {
        const V: &str = "navegador.pausa";
        sem_sobras(V, a, 1)?;
        let motivo = arg_texto(V, a, 0, "motivo")?;
        let inicio = Instant::now();
        let r = ctx.chamar(|v| v.pausa(&motivo));
        // O tempo que o usuário levou não conta no tempo total do roteiro.
        ctx.controle.somar_pausa(inicio.elapsed());
        r
    })?;

    adicionar(scope, &nav, "registrar", move |a| {
        const V: &str = "navegador.registrar";
        sem_sobras(V, a, 1)?;
        let mensagem = arg_texto(V, a, 0, "mensagem")?;
        let mensagem: String = mensagem.chars().take(REGISTRO_MAXIMO).collect();
        ctx.chamar(|v| v.registrar(&mensagem))?;
        ctx.saida.borrow_mut().registros.push(mensagem);
        Ok(())
    })?;

    adicionar(scope, &util, "hoje", move |a| {
        const V: &str = "util.hoje";
        sem_sobras(V, a, 0)?;
        ctx.chamar(|v| v.hoje())
    })?;

    adicionar(scope, &util, "somar_meses", move |a| {
        const V: &str = "util.somar_meses";
        sem_sobras(V, a, 2)?;
        let data = arg_texto(V, a, 0, "data")?;
        let meses = arg_meses(V, a.get(1))?;
        ctx.chamar(|v| v.somar_meses(&data, meses))
    })?;

    nav.set_readonly(true);
    util.set_readonly(true);
    let globais = lua.globals();
    globais.set("navegador", nav)?;
    globais.set("util", util)?;
    Ok(())
}

/// `opcoes` do código por e-mail: nada, um número de segundos, ou `{ janela_segundos = n }`.
/// Qualquer outra chave é recusada: remetente e formato vêm do manifesto, não do script.
fn janela_do_email(verbo: &str, v: Option<&Value>) -> Result<Duration> {
    match v {
        None => Ok(JANELA_PADRAO),
        Some(Value::Table(t)) => {
            let mut janela = JANELA_PADRAO;
            for par in t.pairs::<Value, Value>() {
                let (chave, valor) = par?;
                match &chave {
                    Value::String(s) if s.as_bytes().as_ref() == b"janela_segundos" => {
                        janela = segundos_limitados(
                            verbo,
                            2,
                            "opcoes.janela_segundos",
                            &valor,
                            JANELA_MAXIMA,
                        )?;
                    }
                    _ => {
                        return Err(Error::runtime(format!(
                            "{verbo}: opção desconhecida em opcoes; só `janela_segundos` existe \
                             (o remetente e o formato do código vêm do manifesto)"
                        )));
                    }
                }
            }
            Ok(janela)
        }
        Some(outro) => match como_f64(outro) {
            Some(_) => segundos_limitados(verbo, 2, "janela_segundos", outro, JANELA_MAXIMA),
            None => Err(erro_arg(
                verbo,
                2,
                "opcoes",
                "uma tabela { janela_segundos = n }",
                outro,
            )),
        },
    }
}

fn arg_meses(verbo: &str, v: Option<&Value>) -> Result<i32> {
    let Some(v) = v else {
        return Err(Error::runtime(format!(
            "{verbo}: falta o argumento 2 (n), que deve ser um número inteiro de meses"
        )));
    };
    let n = match v {
        Value::Integer(i) => *i,
        Value::Number(f) if f.is_finite() && f.fract() == 0.0 && f.abs() <= 1e9 => *f as i64,
        outro => return Err(erro_arg(verbo, 2, "n", "um número inteiro de meses", outro)),
    };
    if n.abs() > MESES_MAXIMOS {
        return Err(Error::runtime(format!(
            "{verbo}: n deve estar entre -{MESES_MAXIMOS} e {MESES_MAXIMOS}"
        )));
    }
    i32::try_from(n).map_err(|_| Error::runtime(format!("{verbo}: n fora do intervalo")))
}
