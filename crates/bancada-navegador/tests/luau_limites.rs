//! Os tetos do VM: instruções, memória, tempo, chamadas e tamanho do fonte.

mod luau_comum;

use std::time::Duration;

use bancada_navegador::luau::{ErroDeRoteiro, Limites, Verbos, executar};
use luau_comum::{Chamada, VerbosFalsos};

#[test]
fn laco_infinito_estoura_instrucoes() {
    let mut v = VerbosFalsos::novo();
    let limites = Limites {
        instrucoes: 100_000,
        ..Limites::default()
    };
    let erro = executar("while true do end", &mut v, &limites).expect_err("laço");
    assert_eq!(erro, ErroDeRoteiro::LimiteDeInstrucoes(100_000));
}

#[test]
fn laco_infinito_dentro_de_pcall_tambem_termina() {
    let mut v = VerbosFalsos::novo();
    let limites = Limites {
        instrucoes: 100_000,
        ..Limites::default()
    };
    let erro = executar(
        "while true do pcall(function() while true do end end) end",
        &mut v,
        &limites,
    )
    .expect_err("laço");
    assert_eq!(erro, ErroDeRoteiro::LimiteDeInstrucoes(100_000));
}

#[test]
fn recursao_infinita_termina() {
    let mut v = VerbosFalsos::novo();
    let erro = executar(
        "local function f() return 1 + f() end f()",
        &mut v,
        &Limites::default(),
    )
    .expect_err("recursão");
    assert!(
        matches!(
            erro,
            ErroDeRoteiro::Execucao { .. } | ErroDeRoteiro::LimiteDeInstrucoes(_)
        ),
        "{erro:?}"
    );
}

#[test]
fn laco_infinito_estoura_o_tempo_quando_as_instrucoes_sobram() {
    let mut v = VerbosFalsos::novo();
    let limites = Limites {
        instrucoes: u64::MAX,
        tempo_total: Duration::from_millis(100),
        ..Limites::default()
    };
    let erro = executar("while true do end", &mut v, &limites).expect_err("tempo");
    assert_eq!(
        erro,
        ErroDeRoteiro::LimiteDeTempo(Duration::from_millis(100))
    );
}

#[test]
fn string_rep_gigante_estoura_a_memoria() {
    let mut v = VerbosFalsos::novo();
    let erro = executar(
        r##"local s = string.rep("x", 1000) while true do s = s .. s end"##,
        &mut v,
        &Limites::default(),
    )
    .expect_err("memória");
    assert!(
        matches!(erro, ErroDeRoteiro::LimiteDeMemoria(_)),
        "{erro:?}"
    );
}

#[test]
fn acumular_tabelas_estoura_a_memoria() {
    let mut v = VerbosFalsos::novo();
    let limites = Limites {
        memoria: 4 * 1024 * 1024,
        ..Limites::default()
    };
    let erro = executar(
        "local t = {} for i = 1, 1e8 do t[i] = { i } end",
        &mut v,
        &limites,
    )
    .expect_err("memória");
    assert!(
        matches!(erro, ErroDeRoteiro::LimiteDeMemoria(_)),
        "{erro:?}"
    );
}

#[test]
fn fonte_maior_que_64_kib_e_recusado_sem_executar() {
    let mut v = VerbosFalsos::novo();
    let fonte = format!("navegador.registrar('x') --{}", "a".repeat(64 * 1024));
    let erro = executar(&fonte, &mut v, &Limites::default()).expect_err("grande");
    assert_eq!(
        erro,
        ErroDeRoteiro::FonteGrande {
            tamanho: fonte.len(),
            maximo: 64 * 1024
        }
    );
    assert!(v.chamadas.is_empty());
}

#[test]
fn fonte_no_limite_exato_passa() {
    let mut v = VerbosFalsos::novo();
    let base = "navegador.registrar('x') --";
    let fonte = format!("{base}{}", "a".repeat(64 * 1024 - base.len()));
    assert_eq!(fonte.len(), 64 * 1024);
    executar(&fonte, &mut v, &Limites::default()).expect("no limite");
}

#[test]
fn excesso_de_chamadas_a_verbos_termina() {
    let mut v = VerbosFalsos::novo();
    let limites = Limites {
        chamadas: 5,
        ..Limites::default()
    };
    let erro = executar(
        "for i = 1, 100 do navegador.registrar('i') end",
        &mut v,
        &limites,
    )
    .expect_err("chamadas");
    assert_eq!(erro, ErroDeRoteiro::LimiteDeChamadas(5));
    assert_eq!(v.chamadas.len(), 5, "a sexta chamada nem chega ao app");
}

#[test]
fn excesso_de_chamadas_nao_se_captura_com_pcall() {
    let mut v = VerbosFalsos::novo();
    let limites = Limites {
        chamadas: 2,
        ..Limites::default()
    };
    let erro = executar(
        "for i = 1, 10 do pcall(navegador.registrar, 'i') end navegador.registrar('fim')",
        &mut v,
        &limites,
    )
    .expect_err("chamadas");
    assert_eq!(erro, ErroDeRoteiro::LimiteDeChamadas(2));
    assert_eq!(v.chamadas.len(), 2);
}

#[test]
fn util_tambem_conta_como_chamada() {
    let mut v = VerbosFalsos::novo();
    let limites = Limites {
        chamadas: 3,
        ..Limites::default()
    };
    let erro =
        executar("for i = 1, 10 do util.hoje() end", &mut v, &limites).expect_err("chamadas");
    assert_eq!(erro, ErroDeRoteiro::LimiteDeChamadas(3));
}

/// Verbos que demoram a pausa: o tempo esperando o usuário não conta no tempo total.
struct PausaLenta(VerbosFalsos);

impl Verbos for PausaLenta {
    fn pausa(&mut self, motivo: &str) -> Result<(), bancada_navegador::luau::ErroDeVerbo> {
        std::thread::sleep(Duration::from_millis(300));
        self.0.pausa(motivo)
    }
    fn abrir(&mut self, u: &str) -> Result<(), bancada_navegador::luau::ErroDeVerbo> {
        self.0.abrir(u)
    }
    fn esperar(
        &mut self,
        s: &str,
        p: Duration,
    ) -> Result<bool, bancada_navegador::luau::ErroDeVerbo> {
        self.0.esperar(s, p)
    }
    fn existe(&mut self, s: &str) -> Result<bool, bancada_navegador::luau::ErroDeVerbo> {
        self.0.existe(s)
    }
    fn clicar(&mut self, s: &str) -> Result<(), bancada_navegador::luau::ErroDeVerbo> {
        self.0.clicar(s)
    }
    fn texto(&mut self, s: &str) -> Result<String, bancada_navegador::luau::ErroDeVerbo> {
        self.0.texto(s)
    }
    fn tabela(
        &mut self,
        s: &str,
    ) -> Result<Vec<Vec<String>>, bancada_navegador::luau::ErroDeVerbo> {
        self.0.tabela(s)
    }
    fn preencher(&mut self, s: &str, t: &str) -> Result<(), bancada_navegador::luau::ErroDeVerbo> {
        self.0.preencher(s, t)
    }
    fn preencher_senha(
        &mut self,
        s: &str,
        r: &str,
    ) -> Result<(), bancada_navegador::luau::ErroDeVerbo> {
        self.0.preencher_senha(s, r)
    }
    fn preencher_cpf(&mut self, s: &str) -> Result<(), bancada_navegador::luau::ErroDeVerbo> {
        self.0.preencher_cpf(s)
    }
    fn preencher_codigo_do_email(
        &mut self,
        s: &str,
        j: Duration,
    ) -> Result<(), bancada_navegador::luau::ErroDeVerbo> {
        self.0.preencher_codigo_do_email(s, j)
    }
    fn baixar(
        &mut self,
        s: &str,
        n: Option<&str>,
    ) -> Result<String, bancada_navegador::luau::ErroDeVerbo> {
        self.0.baixar(s, n)
    }
    fn registrar(&mut self, m: &str) -> Result<(), bancada_navegador::luau::ErroDeVerbo> {
        self.0.registrar(m)
    }
    fn hoje(&mut self) -> Result<String, bancada_navegador::luau::ErroDeVerbo> {
        self.0.hoje()
    }
    fn somar_meses(
        &mut self,
        d: &str,
        n: i32,
    ) -> Result<String, bancada_navegador::luau::ErroDeVerbo> {
        self.0.somar_meses(d, n)
    }
}

#[test]
fn tempo_de_pausa_nao_conta_no_tempo_total() {
    let mut v = PausaLenta(VerbosFalsos::novo());
    let limites = Limites {
        tempo_total: Duration::from_millis(150),
        ..Limites::default()
    };
    executar(
        "navegador.pausa('a') navegador.pausa('b') navegador.registrar('ok')",
        &mut v,
        &limites,
    )
    .expect("600 ms de pausa não estouram 150 ms de tempo total");
    assert!(v.0.chamadas.contains(&Chamada::Registrar("ok".into())));
}

#[test]
fn pausa_automatica_dentro_dos_verbos_nao_conta_no_tempo_total() {
    // Três verbos que pausam 400 ms cada (CAPTCHA detectado dentro deles) num teto de 500 ms.
    let mut v = VerbosFalsos::novo().com_pausa_automatica(Duration::from_millis(400));
    let limites = Limites {
        tempo_total: Duration::from_millis(500),
        ..Limites::default()
    };
    executar(
        r##"
        navegador.abrir("https://portal.exemplo.test/entrar")
        navegador.clicar("#entrar")
        navegador.esperar("#saudacao")
        util.hoje()
        navegador.registrar("ok")
        "##,
        &mut v,
        &limites,
    )
    .expect("1,2 s de pausa automática não estouram 500 ms de tempo total");
}

#[test]
fn sem_o_desconto_o_mesmo_roteiro_estouraria_o_tempo() {
    // Controle do teste acima: a mesma espera, sem o app informar a pausa, estoura.
    struct SemDesconto(VerbosFalsos);
    impl Verbos for SemDesconto {
        fn abrir(&mut self, u: &str) -> Result<(), bancada_navegador::luau::ErroDeVerbo> {
            self.0.abrir(u)
        }
        fn esperar(
            &mut self,
            s: &str,
            p: Duration,
        ) -> Result<bool, bancada_navegador::luau::ErroDeVerbo> {
            self.0.esperar(s, p)
        }
        fn existe(&mut self, s: &str) -> Result<bool, bancada_navegador::luau::ErroDeVerbo> {
            self.0.existe(s)
        }
        fn clicar(&mut self, s: &str) -> Result<(), bancada_navegador::luau::ErroDeVerbo> {
            self.0.clicar(s)
        }
        fn texto(&mut self, s: &str) -> Result<String, bancada_navegador::luau::ErroDeVerbo> {
            self.0.texto(s)
        }
        fn tabela(
            &mut self,
            s: &str,
        ) -> Result<Vec<Vec<String>>, bancada_navegador::luau::ErroDeVerbo> {
            self.0.tabela(s)
        }
        fn preencher(
            &mut self,
            s: &str,
            t: &str,
        ) -> Result<(), bancada_navegador::luau::ErroDeVerbo> {
            self.0.preencher(s, t)
        }
        fn preencher_senha(
            &mut self,
            s: &str,
            r: &str,
        ) -> Result<(), bancada_navegador::luau::ErroDeVerbo> {
            self.0.preencher_senha(s, r)
        }
        fn preencher_cpf(&mut self, s: &str) -> Result<(), bancada_navegador::luau::ErroDeVerbo> {
            self.0.preencher_cpf(s)
        }
        fn preencher_codigo_do_email(
            &mut self,
            s: &str,
            j: Duration,
        ) -> Result<(), bancada_navegador::luau::ErroDeVerbo> {
            self.0.preencher_codigo_do_email(s, j)
        }
        fn baixar(
            &mut self,
            s: &str,
            n: Option<&str>,
        ) -> Result<String, bancada_navegador::luau::ErroDeVerbo> {
            self.0.baixar(s, n)
        }
        fn pausa(&mut self, m: &str) -> Result<(), bancada_navegador::luau::ErroDeVerbo> {
            self.0.pausa(m)
        }
        fn registrar(&mut self, m: &str) -> Result<(), bancada_navegador::luau::ErroDeVerbo> {
            self.0.registrar(m)
        }
        fn hoje(&mut self) -> Result<String, bancada_navegador::luau::ErroDeVerbo> {
            self.0.hoje()
        }
        fn somar_meses(
            &mut self,
            d: &str,
            n: i32,
        ) -> Result<String, bancada_navegador::luau::ErroDeVerbo> {
            self.0.somar_meses(d, n)
        }
    }
    let mut v = SemDesconto(VerbosFalsos::novo().com_pausa_automatica(Duration::from_millis(400)));
    let limites = Limites {
        tempo_total: Duration::from_millis(500),
        ..Limites::default()
    };
    let erro = executar(
        r##"
        navegador.abrir("https://portal.exemplo.test/entrar")
        navegador.clicar("#entrar")
        navegador.esperar("#saudacao")
        util.hoje()
        "##,
        &mut v,
        &limites,
    )
    .expect_err("sem o desconto estoura");
    assert_eq!(
        erro,
        ErroDeRoteiro::LimiteDeTempo(Duration::from_millis(500))
    );
}

#[test]
fn limites_padrao_sao_sensatos() {
    let l = Limites::default();
    assert_eq!(l.tamanho_do_fonte, 64 * 1024);
    assert!(l.instrucoes >= 1_000_000);
    assert!(l.memoria >= 8 * 1024 * 1024);
    assert!(l.chamadas >= 100);
    assert!(l.tempo_total >= Duration::from_secs(60));
}
