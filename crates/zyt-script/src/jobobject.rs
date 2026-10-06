//! The group every child of one script is kept in.
//!
//! A process group is enough on unix: a signal to the group reaches the shell
//! and everything it started. Windows has no such thing — `taskkill /T` walks
//! the tree by parent, and a child that detached itself is not in that tree
//! any more — so there the children are assigned to a job object, which holds
//! them whatever they do with their parentage. The job is set to kill what is
//! left when its handle closes, so a script that was given up on cannot leave
//! a program behind.
//!
//! On unix this is nothing: the group is asked for when the command is built
//! (`own_group`) and signalled by `stop_group`.

use crate::error::Result;

/// The group the children of one script belong to.
#[derive(Debug)]
pub struct JobGroup {
    #[cfg_attr(not(windows), allow(dead_code))]
    handle: usize,
}

#[cfg(not(windows))]
impl JobGroup {
    /// A group for one script.
    pub fn new() -> Result<Self> {
        Ok(Self { handle: 0 })
    }

    /// Puts a child in the group.
    pub fn adopt(&self, child: &std::process::Child) -> Result<()> {
        let _ = child;
        Ok(())
    }

    /// Kills everything left in the group.
    pub fn terminate(&self) {}
}

#[cfg(windows)]
impl JobGroup {
    /// A group for one script, killing what is left when it is dropped.
    pub fn new() -> Result<Self> {
        use windows::Win32::System::JobObjects::{
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JobObjectExtendedLimitInformation, SetInformationJobObject,
        };

        let job = unsafe { windows::Win32::System::JobObjects::CreateJobObjectW(None, None) }
            .map_err(|error| crate::ScriptError::Group {
                source: std::io::Error::other(error),
            })?;

        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let set = unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                std::ptr::from_ref(&limits).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if let Err(error) = set {
            unsafe {
                let _ = windows::Win32::Foundation::CloseHandle(job);
            }
            return Err(crate::ScriptError::Group {
                source: std::io::Error::other(error),
            });
        }

        Ok(Self {
            handle: job.0 as usize,
        })
    }

    /// Puts a child in the group, so it is killed with everything else in it.
    pub fn adopt(&self, child: &std::process::Child) -> Result<()> {
        use std::os::windows::io::AsRawHandle;
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::System::JobObjects::AssignProcessToJobObject;

        let job = HANDLE(self.handle as *mut std::ffi::c_void);
        let process = HANDLE(child.as_raw_handle());
        unsafe { AssignProcessToJobObject(job, process) }.map_err(|error| {
            crate::ScriptError::Group {
                source: std::io::Error::other(error),
            }
        })
    }

    /// Kills everything left in the group.
    pub fn terminate(&self) {
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::System::JobObjects::TerminateJobObject;

        let job = HANDLE(self.handle as *mut std::ffi::c_void);
        unsafe {
            let _ = TerminateJobObject(job, 1);
        }
    }
}

#[cfg(windows)]
impl Drop for JobGroup {
    fn drop(&mut self) {
        use windows::Win32::Foundation::{CloseHandle, HANDLE};

        let job = HANDLE(self.handle as *mut std::ffi::c_void);
        unsafe {
            let _ = CloseHandle(job);
        }
    }
}
