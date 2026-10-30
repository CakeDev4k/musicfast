//! Filhos (yt-dlp) presos a um Job Object que morre com o app.
//!
//! Trazido do `prime_win::job` do Produtividade Prime, reduzido a um job só
//! para o app inteiro: se o MusicFast fecha, trava ou é morto pelo
//! Gerenciador de Tarefas, o Windows encerra o yt-dlp e o Deno que ele abre
//! (OTIMIZACAO.md §5.5: processo órfão segura arquivo e CPU). O filho nasce
//! suspenso e só roda depois de estar no job, para que nenhum neto escape.

#[cfg(windows)]
use std::process::Child;
use std::process::{Command, Stdio};
#[cfg(windows)]
use std::sync::OnceLock;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
#[cfg(windows)]
const CREATE_SUSPENDED: u32 = 0x0000_0004;


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
    };
    use windows_sys::Win32::System::Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME};

    pub struct Job(pub HANDLE);
    // SAFETY: o handle de um job é um valor do kernel usável de qualquer thread.
    unsafe impl Send for Job {}
    unsafe impl Sync for Job {}

    pub fn create() -> std::io::Result<Job> {
        // SAFETY: job anônimo com atributos padrão; o handle é conferido.
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if job.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: estrutura zerada com só o limite que queremos.
        let ok = unsafe {
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION as *const c_void,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if ok == 0 {
            // SAFETY: o handle é nosso e fecha uma vez.
            unsafe { CloseHandle(job) };
            return Err(std::io::Error::last_os_error());
        }
        Ok(Job(job))
    }

    pub fn assign(job: &Job, process: HANDLE) -> std::io::Result<()> {
        // SAFETY: o processo está vivo (suspenso) durante a chamada.
        if unsafe { AssignProcessToJobObject(job.0, process) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
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

/// Roda até o fim, sem janela e dentro do
/// job. Devolve a saída como `Command::output`. Prioridade normal: quem
/// espera a resolução é a pessoa que clicou em tocar.
pub fn output(mut cmd: Command) -> std::io::Result<std::process::Output> {
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use std::os::windows::process::CommandExt;
        let Some(job) = job() else {
            cmd.creation_flags(CREATE_NO_WINDOW);
            return cmd.output();
        };
        cmd.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
        let mut child: Child = cmd.spawn()?;
        let started = win::assign(job, child.as_raw_handle() as _).and_then(|()| win::resume(child.id()));
        if let Err(error) = started {
            // Um filho suspenso para sempre deixaria quem chamou esperando.
            let _ = child.kill();
            return Err(error);
        }
        child.wait_with_output()
    }
    #[cfg(not(windows))]
    {
        cmd.output()
    }
}
