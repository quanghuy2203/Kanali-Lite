use std::{
    env,
    ffi::{c_void, CString},
    mem::transmute_copy,
    path::Path,
};

#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryA(name: *const i8) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const i8) -> *mut c_void;
    fn FreeLibrary(module: *mut c_void) -> i32;
}

type Init = extern "system" fn(u32) -> u8;
type Shutdown = extern "system" fn();
type Quick = extern "system" fn(u8, u8) -> u8;
type ReadByte = extern "system" fn(u8, u8, u8, *mut u8) -> u8;
type WriteByte = extern "system" fn(u8, u8, u8, u8) -> u8;
type WriteWord = extern "system" fn(u8, u8, u8, u16) -> u8;
type WriteBlock = extern "system" fn(u8, u8, u8, *const u8, u8) -> u8;

struct Ene {
    module: *mut c_void,
    shutdown: Shutdown,
    quick: Quick,
    read_byte: ReadByte,
    write_byte: WriteByte,
    write_word: WriteWord,
    write_block: WriteBlock,
}

impl Ene {
    unsafe fn load(path: &Path) -> Result<Self, String> {
        let raw = CString::new(path.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
        let module = LoadLibraryA(raw.as_ptr());
        if module.is_null() {
            return Err("Không nạp được MsIo32_ENE.dll".into());
        }
        unsafe fn sym<T>(module: *mut c_void, name: &str) -> Result<T, String> {
            let name = CString::new(name).unwrap();
            let ptr = GetProcAddress(module, name.as_ptr());
            if ptr.is_null() {
                return Err(format!("DLL thiếu {name:?}"));
            }
            Ok(transmute_copy(&ptr))
        }
        let init: Init = sym(module, "InitializeSMB")?;
        if init(0) == 0 {
            FreeLibrary(module);
            return Err("ENE SMBus không khởi tạo được".into());
        }
        Ok(Self {
            module,
            shutdown: sym(module, "ShutdownSMB")?,
            quick: sym(module, "Read_SMB_Quick")?,
            read_byte: sym(module, "Read_SMB_Byte")?,
            write_byte: sym(module, "Write_SMB_Byte")?,
            write_word: sym(module, "Write_SMB_Word")?,
            write_block: sym(module, "Write_SMB_Block")?,
        })
    }

    fn read_raw(&self, addr: u8, command: u8) -> Result<u8, String> {
        let mut value = 0u8;
        if (self.read_byte)(0, addr, command, &mut value) == 0 {
            Err("SMBus read thất bại".into())
        } else {
            Ok(value)
        }
    }
    fn point(&self, addr: u8, reg: u16) -> Result<(), String> {
        // MsIo32_ENE serializes the u16 value in SMBus little-endian order.
        // Passing the numeric register unchanged produces the required wire
        // bytes (for example 0x8021 -> 21 80). Swapping here addresses 0x2180.
        if (self.write_word)(0, addr, 0, reg) == 0 {
            Err("SMBus pointer thất bại".into())
        } else {
            Ok(())
        }
    }
    fn read_reg(&self, addr: u8, reg: u16) -> Result<u8, String> {
        self.point(addr, reg)?;
        self.read_raw(addr, 0x81)
    }
    fn write_reg(&self, addr: u8, reg: u16, value: u8) -> Result<(), String> {
        self.point(addr, reg)?;
        if (self.write_byte)(0, addr, 1, value) == 0 {
            Err("SMBus write thất bại".into())
        } else {
            Ok(())
        }
    }
    fn block(&self, addr: u8, reg: u16, data: &[u8]) -> Result<(), String> {
        for (chunk_index, chunk) in data.chunks(32).enumerate() {
            self.point(addr, reg + (chunk_index * 32) as u16)?;
            if (self.write_block)(0, addr, 3, chunk.as_ptr(), chunk.len() as u8) == 0 {
                for (i, value) in chunk.iter().enumerate() {
                    self.write_reg(addr, reg + (chunk_index * 32 + i) as u16, *value)?;
                }
            }
        }
        Ok(())
    }
    fn verified(&self, addr: u8) -> bool {
        (self.quick)(0, addr) != 0
            && (0..16u8).all(|i| self.read_raw(addr, 0xA0 + i).ok() == Some(i))
            && (0..6u16)
                .map(|i| self.read_reg(addr, 0x1030 + i).unwrap_or(0))
                .collect::<Vec<_>>()
                != b"Micron"
    }
    fn layout(&self, addr: u8) -> Result<(u16, u16, usize), String> {
        let version: Vec<u8> = (0..16u16)
            .map(|i| self.read_reg(addr, 0x1000 + i))
            .collect::<Result<_, _>>()?;
        let version = String::from_utf8_lossy(&version)
            .trim_matches(char::from(0))
            .to_string();
        let config: Vec<u8> = (0..64u16)
            .map(|i| self.read_reg(addr, 0x1C00 + i))
            .collect::<Result<_, _>>()?;
        let v2 = version.starts_with("AUDA0-E6K5") || version.starts_with("AUMA0-E6K5");
        let count = usize::from(config[2].max(config[3])).min(30);
        println!(
            "addr=0x{addr:02X} firmware={version:?} config02={} config03={} leds={count} layout={}",
            config[2],
            config[3],
            if v2 { "v2" } else { "v1" }
        );
        if count == 0 {
            return Err(format!("ENE {addr:02X} không báo số LED"));
        }
        Ok(if v2 {
            (0x8100, 0x8160, count)
        } else {
            (0x8000, 0x8010, count)
        })
    }
    fn prepare(
        &self,
        addr: u8,
        layout: (u16, u16, usize),
        effect: &str,
        rgb: [u8; 3],
    ) -> Result<(), String> {
        let (direct, effect_reg, count) = layout;
        let wire: Vec<u8> = (0..count).flat_map(|_| [rgb[0], rgb[2], rgb[1]]).collect();
        match effect {
            "static" => {
                self.write_reg(addr, 0x8021, 1)?;
                self.write_reg(addr, 0x80A0, 1)?;
                self.write_reg(addr, 0x8020, 1)?;
                self.write_reg(addr, 0x80A0, 1)?;
                self.block(addr, direct, &wire)?;
                self.write_reg(addr, 0x8020, 1)?;
            }
            mode => {
                self.write_reg(addr, 0x8020, 0)?;
                self.write_reg(addr, 0x80A0, 1)?;
                if mode == "breathing" {
                    self.block(addr, effect_reg, &wire)?;
                    self.write_reg(addr, 0x80A0, 1)?;
                }
                let id = match mode {
                    "off" => 0,
                    "breathing" => 2,
                    "rainbow" => 5,
                    _ => return Err("Hiệu ứng ENE không hợp lệ".into()),
                };
                self.write_reg(addr, 0x8021, id)?;
                self.write_reg(addr, 0x8022, 2)?;
                self.write_reg(addr, 0x8023, 0)?;
            }
        }
        Ok(())
    }

    fn commit(&self, addr: u8) -> Result<(), String> {
        self.write_reg(addr, 0x80A0, 1)
    }
}

impl Drop for Ene {
    fn drop(&mut self) {
        (self.shutdown)();
        unsafe {
            FreeLibrary(self.module);
        }
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 4 {
        return Err("Dùng: helper <effect> <RRGGBB> <dll>".into());
    }
    let color = u32::from_str_radix(&args[2], 16).map_err(|_| "Màu không hợp lệ")?;
    let rgb = [(color >> 16) as u8, (color >> 8) as u8, color as u8];
    let ene = unsafe { Ene::load(Path::new(&args[3]))? };
    let addresses = [0x71, 0x73];
    // Complete slow probing before changing visible state. Stage both DIMMs,
    // then issue their final APPLY writes back-to-back.
    let mut layouts = Vec::with_capacity(addresses.len());
    for addr in addresses {
        if !ene.verified(addr) {
            return Err(format!(
                "0x{addr:02X} không phải controller ENE đã xác minh"
            ));
        }
        layouts.push(ene.layout(addr)?);
    }
    for (addr, layout) in addresses.into_iter().zip(layouts) {
        ene.prepare(addr, layout, &args[1], rgb)?;
    }
    for addr in addresses {
        ene.commit(addr)?;
    }
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
