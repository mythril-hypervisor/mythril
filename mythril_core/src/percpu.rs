use alloc::vec::Vec;

pub struct PerCpu<T> {
    val: Option<Vec<T>>,
}

//FIXME: the interface for this should be reworked
impl<T> PerCpu<T>
where
    T: Default,
{
    fn init_once(&mut self) {
        match self.val {
            Some(_) => return,
            None => {
                //TODO: we should have a default for each CPU
                let mut vals = vec![];
                vals.push(Default::default());
                self.val = Some(vals);
            }
        }
    }

    fn cpuid() -> usize {
        //TODO: this should be from CPUID
        0
    }

    pub const fn new() -> Self {
        Self { val: None }
    }

    pub fn get(&self) -> &T {
        let per_cpu = self
            .val
            .as_ref()
            .expect("Attempt to get per-cpu without set");
        &per_cpu[Self::cpuid()]
    }

    pub fn get_mut(&mut self) -> &mut T {
        let per_cpu = self
            .val
            .as_mut()
            .expect("Attempt to get per-cpu without set");
        &mut per_cpu[Self::cpuid()]
    }

    pub fn set(&mut self, val: T) {
        self.init_once();
        let per_cpu = self
            .val
            .as_mut()
            .expect("Failed to access per-cpu value after init");
        per_cpu[Self::cpuid()] = val;
    }
}
