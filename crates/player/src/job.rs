//! Filhos (yt-dlp) presos a um Job Object que morre com o app.
//!
//! Trazido do `prime_win::job` do Produtividade Prime, reduzido a um job só
//! para o app inteiro: se o MusicFast fecha, trava ou é morto pelo
//! Gerenciador de Tarefas, o Windows encerra o yt-dlp e o Deno que ele abre
//! (OTIMIZACAO.md §5.5: processo órfão segura arquivo e CPU). O filho nasce
//! suspenso e só roda depois de estar no job, para que nenhum neto escape.
//!
//! Cada chamada tem prazo (`output`): um yt-dlp travado (rede engasgada, um
//! desafio de JavaScript que não termina) deixava a música em "Buscando o
//! áudio" para sempre. Passado o prazo, sai o filho e tudo o que ele abriu,
//! por um segundo job só dele, dentro do do app.

use std::io::Read;
use std::process::{Child, Command, Output, Stdio};
#[cfg(windows)]
use std::sync::OnceLock;
use std::time::{Duration, Instant};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
#[cfg(windows)]
const CREATE_SUSPENDED: u32 = 0x0000_0004;
/// De quanto em quanto tempo o filho é conferido: no máximo isto a mais
/// numa resolução de 1–4 s.
const POLL: Duration = Duration::from_millis(20);

#[cfg(windows)]
mod win {
    use std::ffi::c_void;
    use std::mem::size_of;

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
    };
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation, SetInformationJobObject,
        TerminateJobObject,
    };
    use windows_sys::Win32::System::Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME};

    pub struct Job(pub HANDLE);
    // SAFETY: o handle de um job é um valor do kernel usável de qualquer thread.
    unsafe impl Send for Job {}
    unsafe impl Sync for Job {}

    impl Drop for Job {
        /// Fechar o último handle encerra o que ainda estiver no job
        /// (`KILL_ON_JOB_CLOSE`): o neto que sobrou de uma chamada sai junto.
        fn drop(&mut self) {
            // SAFETY: o handle é nosso e fecha uma vez.
            unsafe { CloseHandle(self.0) };
        }
    }

    pub fn create() -> std::io::Result<Job> {
        // SAFETY: job anônimo com atributos padrão; o handle é conferido.
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if job.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        let job = Job(job);
        // SAFETY: estrutura zerada com só o limite que queremos.
        let ok = unsafe {
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                &limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION as *const c_void,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if ok == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(job)
    }

    pub fn assign(job: &Job, process: HANDLE) -> std::io::Result<()> {
        // SAFETY: o processo está vivo (suspenso) durante a chamada.
        if unsafe { AssignProcessToJobObject(job.0, process) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }

    /// Encerra tudo o que está no job.
    pub fn terminate(job: &Job) {
        // SAFETY: o handle é nosso e está aberto.
        unsafe { TerminateJobObject(job.0, 1) };
    }

    /// Retoma as threads de um processo criado suspenso (o `std::process`
    /// não devolve o handle da thread principal; a lista do sistema devolve).
    pub fn resume(pid: u32) -> std::io::Result<()> {
        // SAFETY: instantâneo das threads; handles conferidos e fechados.
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
            if snapshot == INVALID_HANDLE_VALUE {
                return Err(std::io::Error::last_os_error());
            }
            let mut entry: THREADENTRY32 = std::mem::zeroed();
            entry.dwSize = size_of::<THREADENTRY32>() as u32;
            let mut resumed = 0;
            let mut more = Thread32First(snapshot, &mut entry) != 0;
            while more {
                if entry.th32OwnerProcessID == pid {
                    let thread = OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID);
                    if !thread.is_null() {
                        if ResumeThread(thread) != u32::MAX {
                            resumed += 1;
                        }
                        CloseHandle(thread);
                    }
                }
                more = Thread32Next(snapshot, &mut entry) != 0;
            }
            CloseHandle(snapshot);
            if resumed == 0 {
                return Err(std::io::Error::other("nenhuma thread do processo foi retomada"));
            }
        }
        Ok(())
    }
}

#[cfg(windows)]
fn job() -> Option<&'static win::Job> {
    static JOB: OnceLock<Option<win::Job>> = OnceLock::new();
    JOB.get_or_init(|| match win::create() {
        Ok(job) => Some(job),
        Err(error) => {
            log::warn!("job object indisponível, o yt-dlp roda sem ele: {error}");
            None
        }
    })
    .as_ref()
}

/// Roda até o fim, sem janela e dentro do job, e devolve a saída como
/// `Command::output`. Passando de `limit`, encerra o filho e os netos dele e
/// devolve `ErrorKind::TimedOut`. Prioridade normal: quem espera a resolução
/// é a pessoa que clicou em tocar.
pub fn output(mut cmd: Command, limit: Duration) -> std::io::Result<Output> {
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use std::os::windows::process::CommandExt;
        let Some(job) = job() else {
            cmd.creation_flags(CREATE_NO_WINDOW);
            let mut child = cmd.spawn()?;
            return collect(&mut child, limit, |child| {
                let _ = child.kill();
            });
        };
        cmd.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
        let mut child: Child = cmd.spawn()?;
        let handle = child.as_raw_handle() as _;
        if let Err(error) = win::assign(job, handle) {
            // Um filho suspenso para sempre deixaria quem chamou esperando.
            let _ = child.kill();
            return Err(error);
        }
        // O job só desta chamada, aninhado no do app (Windows 8 em diante):
        // encerrá-lo leva os netos junto. Sem ele, só o filho sai no prazo.
        let tree = win::create().ok().filter(|tree| win::assign(tree, handle).is_ok());
        if let Err(error) = win::resume(child.id()) {
            let _ = child.kill();
            return Err(error);
        }
        collect(&mut child, limit, |child| match &tree {
            Some(tree) => win::terminate(tree),
            None => {
                let _ = child.kill();
            }
        })
    }
    #[cfg(not(windows))]
    {
        let mut child = cmd.spawn()?;
        collect(&mut child, limit, |child| {
            let _ = child.kill();
        })
    }
}

/// Espera o filho até `limit`, lendo a saída em paralelo (o que o
/// `wait_with_output` faz, sem prazo). `end` encerra o que ele deixou: no
/// prazo, ele e os netos; ao terminar bem, um neto que ainda segure a saída
/// e não deixaria a leitura chegar ao fim.
fn collect(child: &mut Child, limit: Duration, end: impl Fn(&mut Child)) -> std::io::Result<Output> {
    fn read(pipe: Option<impl Read + Send + 'static>) -> std::thread::JoinHandle<Vec<u8>> {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut buf);
            }
            buf
        })
    }
    let stdout = read(child.stdout.take());
    let stderr = read(child.stderr.take());
    let deadline = Instant::now() + limit;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            end(child);
            let _ = child.wait();
            // As leituras terminam sozinhas quando a saída fecha.
            return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, format!("passou de {} s", limit.as_secs())));
        }
        std::thread::sleep(POLL);
    };
    end(child);
    Ok(Output { status, stdout: stdout.join().unwrap_or_default(), stderr: stderr.join().unwrap_or_default() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shell(script: &str) -> Command {
        if cfg!(windows) {
            let mut cmd = Command::new("cmd");
            cmd.args(["/C", script]);
            cmd
        } else {
            let mut cmd = Command::new("sh");
            cmd.args(["-c", script]);
            cmd
        }
    }

    #[test]
    fn a_quick_child_answers_with_its_output() {
        let out = output(shell("echo pronto"), Duration::from_secs(20)).unwrap();
        assert!(out.status.success());
        assert!(String::from_utf8_lossy(&out.stdout).contains("pronto"));
    }

    #[test]
    fn a_stuck_child_is_ended_at_the_deadline() {
        // O `ping` (ou o `sleep`) é neto: o `cmd` (ou o `sh`) é quem foi aberto.
        let stuck = if cfg!(windows) { "ping -n 30 127.0.0.1 >NUL" } else { "sleep 30" };
        let started = Instant::now();
        let error = output(shell(stuck), Duration::from_millis(500)).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(10), "{:?}", started.elapsed());
    }
}
