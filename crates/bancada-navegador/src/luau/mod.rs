//! Host Luau dos roteiros escritos (ADR-022 do SBF).
//!
//! O roteiro é um programa Luau escrito à mão que só chama VERBOS de alto nível
//! (`navegador.abrir`, `navegador.clicar`, `navegador.preencher_senha`...). Este módulo não
//! sabe de CDP: o app implementa [`Verbos`] por cima do navegador, da lista de bloqueio, do
//! chaveiro e do e-mail, e chama [`executar`].
//!
//! O VM é o sandbox do Luau, sem `os`, `io`, `require`, `loadstring`, `getfenv`, `debug`,
//! `coroutine` nem rede; só as tabelas `navegador` e `util` e as bibliotecas `string`,
//! `table`, `math` e `utf8`. Há teto de instruções, de memória, de tempo total, de chamadas a
//! verbos e de tamanho do fonte ([`Limites`]). O script nunca recebe senha, CPF nem código de
//! MFA: os verbos de segredo só recebem o rótulo do chaveiro.

mod limites;
mod ponte;
mod verbos;

use std::sync::Arc;

use mlua::{Error, Lua, LuaOptions, StdLib, Value, VmState};

pub use limites::{ErroDeRoteiro, Limites, Saida};
pub use ponte::{ESPERA_MAXIMA, ESPERA_PADRAO, JANELA_MAXIMA, JANELA_PADRAO};
pub use verbos::{ErroDeVerbo, ResultadoDeVerbo, Verbos};

use limites::Controle;
use ponte::Contexto;

/// Nome do roteiro nas mensagens de erro do Luau (`roteiro:12: ...`).
const NOME_DO_CHUNK: &str = "roteiro";

/// Globais que o roteiro enxerga além de `navegador` e `util`. Tudo o mais é removido.
const GLOBAIS_PERMITIDOS: &[&str] = &[
    "assert",
    "error",
    "pcall",
    "xpcall",
    "select",
    "type",
    "typeof",
    "tostring",
    "tonumber",
    "ipairs",
    "pairs",
    "next",
    "rawget",
    "rawset",
    "rawequal",
    "rawlen",
    "setmetatable",
    "getmetatable",
    "unpack",
    "string",
    "table",
    "math",
    "utf8",
    "_VERSION",
];

/// Executa um roteiro escrito. Termina no primeiro erro (de sintaxe, de execução, de limite ou
/// de verbo fatal) e devolve o que o roteiro baixou e registrou quando termina bem.
///
/// Cada chamada cria um VM novo: nada de um roteiro vaza para o seguinte.
pub fn executar(
    fonte: &str,
    verbos: &mut dyn Verbos,
    limites: &Limites,
) -> Result<Saida, ErroDeRoteiro> {
    if fonte.len() > limites.tamanho_do_fonte {
        return Err(ErroDeRoteiro::FonteGrande {
            tamanho: fonte.len(),
            maximo: limites.tamanho_do_fonte,
        });
    }
    let controle = Arc::new(Controle::novo(limites.clone()));
    let lua = nova_vm(&controle).map_err(|e| ErroDeRoteiro::Interno(e.to_string()))?;
    let ctx = Contexto::novo(verbos, Arc::clone(&controle));

    let resultado = lua.scope(|escopo| {
        ponte::instalar(&lua, escopo, &ctx)?;
        lua.load(fonte).set_name(format!("={NOME_DO_CHUNK}")).exec()
    });

    match resultado {
        Ok(()) => match controle.tomar_fatal() {
            // O script engoliu o erro com `pcall`, mas o host já tinha decidido encerrar.
            Some(fatal) => Err(fatal),
            None => Ok(ctx.tomar_saida()),
        },
        Err(erro) => Err(converter_erro(&erro, &controle)),
    }
}

/// Monta o VM: bibliotecas seguras, globais enxutos, sandbox, interrupt e teto de memória.
fn nova_vm(controle: &Arc<Controle>) -> mlua::Result<Lua> {
    let lua = Lua::new_with(
        StdLib::STRING | StdLib::TABLE | StdLib::MATH | StdLib::UTF8,
        LuaOptions::default(),
    )?;

    // Allowlist: tudo que não está na lista (require, loadstring, getfenv, setfenv, print,
    // collectgarbage, newproxy, os, io, debug...) some, mesmo que o mlua o tenha posto lá.
    let globais = lua.globals();
    let mut remover = Vec::new();
    for par in globais.pairs::<Value, Value>() {
        let (chave, _) = par?;
        let permitido = match &chave {
            Value::String(s) => GLOBAIS_PERMITIDOS
                .iter()
                .any(|p| s.as_bytes().as_ref() == p.as_bytes()),
            _ => false,
        };
        if !permitido {
            remover.push(chave);
        }
    }
    for chave in remover {
        globais.raw_set(chave, Value::Nil)?;
    }

    lua.sandbox(true)?;

    let c = Arc::clone(controle);
    lua.set_interrupt(move |_| match c.tique() {
        Ok(()) => Ok(VmState::Continue),
        Err(motivo) => Err(Error::runtime(motivo)),
    });

    lua.set_memory_limit(lua.used_memory().saturating_add(controle.limites().memoria))?;
    Ok(lua)
}

/// Traduz um erro do mlua para o que o app entende, sem vazar nada além do que o script (ou o
/// verbo) escreveu.
fn converter_erro(erro: &Error, controle: &Controle) -> ErroDeRoteiro {
    let linha = linha_do_erro(erro);
    if let Some(fatal) = controle.tomar_fatal() {
        return match fatal {
            ErroDeRoteiro::Verbo { erro, linha: None } => ErroDeRoteiro::Verbo { erro, linha },
            outro => outro,
        };
    }
    if let Some(verbo) = erro.downcast_ref::<ErroDeVerbo>() {
        return ErroDeRoteiro::Verbo {
            erro: verbo.clone(),
            linha,
        };
    }
    match raiz(erro) {
        Error::SyntaxError { message, .. } => ErroDeRoteiro::Sintaxe {
            mensagem: sem_prefixo_de_linha(message),
            linha: linha_em_texto(message),
        },
        Error::MemoryError(_) => ErroDeRoteiro::LimiteDeMemoria(controle.limites().memoria),
        Error::RuntimeError(m) => ErroDeRoteiro::Execucao {
            mensagem: sem_prefixo_de_linha(m),
            linha: linha_em_texto(m).or(linha),
        },
        outro => ErroDeRoteiro::Execucao {
            mensagem: outro.to_string(),
            linha,
        },
    }
}

/// O erro mais interno, atravessando `CallbackError` e `WithContext`.
fn raiz(erro: &Error) -> &Error {
    match erro {
        Error::CallbackError { cause, .. } | Error::WithContext { cause, .. } => raiz(cause),
        outro => outro,
    }
}

/// Linha do roteiro: da mensagem (`roteiro:7: ...`) ou do traceback do callback.
fn linha_do_erro(erro: &Error) -> Option<u32> {
    match erro {
        Error::CallbackError { traceback, cause } => {
            linha_em_texto(traceback).or_else(|| linha_do_erro(cause))
        }
        Error::WithContext { cause, .. } => linha_do_erro(cause),
        Error::RuntimeError(m) => linha_em_texto(m),
        Error::SyntaxError { message, .. } => linha_em_texto(message),
        _ => None,
    }
}

/// Acha o primeiro `roteiro:<n>` no texto.
fn linha_em_texto(texto: &str) -> Option<u32> {
    let marca = format!("{NOME_DO_CHUNK}:");
    let mut resto = texto;
    while let Some(i) = resto.find(&marca) {
        let depois = &resto[i + marca.len()..];
        let digitos: String = depois.chars().take_while(char::is_ascii_digit).collect();
        if let Ok(n) = digitos.parse() {
            return Some(n);
        }
        resto = depois;
    }
    None
}

/// Tira o `roteiro:7: ` do começo da mensagem (a linha vai em campo próprio) e o rastro de
/// pilha que o Luau às vezes anexa.
fn sem_prefixo_de_linha(mensagem: &str) -> String {
    let primeira = mensagem.lines().next().unwrap_or("").trim();
    let marca = format!("{NOME_DO_CHUNK}:");
    if let Some(resto) = primeira.strip_prefix(&marca) {
        let sem_numero = resto.trim_start_matches(|c: char| c.is_ascii_digit());
        if let Some(msg) = sem_numero.strip_prefix(':') {
            return msg.trim().to_string();
        }
    }
    primeira.to_string()
}
