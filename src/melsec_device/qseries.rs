pub mod melsec_qseries{

	#[derive(Debug)]
	pub enum DeviceType{
		BIT,
		WORD,
	}
	impl DeviceType{
		// デバイスの種類を文字列で返す
		pub fn to_string(&self) -> &'static str {
			match self {
				DeviceType::BIT => "BIT",
				DeviceType::WORD => "WORD",
			}
		}
	}
	
	#[derive(Debug)]
	pub struct DeviceInfo{
		pub name:&'static str,
		pub code:u8,
		base:u8,
		class:DeviceType,
		description:&'static str,
	}
	
	const DEVICE_LIST:[DeviceInfo; 27]= [
		DeviceInfo{name:"SM",code:0x91,base:10,class:DeviceType::BIT,description:"特殊リレー"},
		DeviceInfo{name:"SD",code:0xA9,base:10,class:DeviceType::WORD,description:"特殊レジスタ"},
		
		DeviceInfo{name:"X",code:0x9C,base:16,class:DeviceType::BIT,description:"入力"},
		DeviceInfo{name:"Y",code:0x9D,base:16,class:DeviceType::BIT,description:"出力"},
		DeviceInfo{name:"M",code:0x90,base:10,class:DeviceType::BIT,description:"内部リレー"},
		DeviceInfo{name:"L",code:0x92,base:10,class:DeviceType::BIT,description:"ラッチリレー"},
		DeviceInfo{name:"F",code:0x93,base:10,class:DeviceType::BIT,description:"アナンシエータ"},
		DeviceInfo{name:"V",code:0x94,base:10,class:DeviceType::BIT,description:"エッジリレー"},
		DeviceInfo{name:"B",code:0xA0,base:16,class:DeviceType::BIT,description:"リンクリレー"},
		
		DeviceInfo{name:"D",code:0xA8,base:10,class:DeviceType::WORD,description:"データレジスタ"},
		DeviceInfo{name:"W",code:0xB4,base:16,class:DeviceType::WORD,description:"リンクレジスタ"},
		
		DeviceInfo{name:"TS",code:0xC1,base:10,class:DeviceType::BIT,description:"タイマ接点"},
		DeviceInfo{name:"TC",code:0xC0,base:10,class:DeviceType::BIT,description:"タイマコイル"},
		DeviceInfo{name:"TN",code:0xC2,base:10,class:DeviceType::WORD,description:"タイマ現在値"},

		DeviceInfo{name:"SS",code:0xC7,base:10,class:DeviceType::BIT,description:"積算タイマ接点"},
		DeviceInfo{name:"SC",code:0xC6,base:10,class:DeviceType::BIT,description:"積算タイマコイル"},
		DeviceInfo{name:"SN",code:0xC8,base:10,class:DeviceType::WORD,description:"積算タイマ現在値"},

		DeviceInfo{name:"CS",code:0xC4,base:10,class:DeviceType::BIT,description:"カウンタ接点"},
		DeviceInfo{name:"CC",code:0xC3,base:10,class:DeviceType::BIT,description:"カウンタコイル"},
		DeviceInfo{name:"CN",code:0xC5,base:10,class:DeviceType::WORD,description:"カウンタ現在値"},
		
		DeviceInfo{name:"SB",code:0xA1,base:16,class:DeviceType::BIT,description:"リンク特殊リレー"},
		DeviceInfo{name:"SW",code:0xB5,base:16,class:DeviceType::WORD,description:"リンク特殊レジスタ"},
		
		DeviceInfo{name:"DX",code:0xA2,base:16,class:DeviceType::BIT,description:"ダイレクトアクセス入力"},
		DeviceInfo{name:"DY",code:0xA3,base:16,class:DeviceType::BIT,description:"ダイレクトアクセス出力"},

		DeviceInfo{name:"Z",code:0xCC,base:10,class:DeviceType::WORD,description:"インデックスレジスタ"},

		DeviceInfo{name:"R",code:0xAF,base:10,class:DeviceType::WORD,description:"ファイルレジスタ"},
		DeviceInfo{name:"ZR",code:0xB0,base:10,class:DeviceType::WORD,description:"ファイルレジスタ"},

	];

	// デバイスの文字列からデバイスコードを取得する。
	// デバイスはD1000のようにデバイス名とアドレスで構成される。
	// アドレスは、10進数または16進数で表現される。
	// 文字列の先頭がデバイス名であることを確認し、デバイス名に対応するデバイスコードを取得する。
	// デバイス名が存在しない場合はNoneを返す。
	// デバイス名は1文字又は2文字である。
	// デバイス名の後にアドレスが続く。
	// アドレスは10進数または16進数で表現される。
	// アドレスはDeviceInfoのbaseで指定された進数で表現される。
	pub fn to_device_code(device:&str)->Option<[u8;4]>{
		// device に含まれるデバイス名をDeviceListのDeciceCodeのnameから検索する。
		// デバイス名が存在しない場合はNoneを返す。
		let param = match DEVICE_LIST.iter().find(|&x| device.starts_with(x.name)){
			None => return None,
			Some(x) => x,
		};
		let address = device.replace(param.name, "");

		// code.baseが10の場合はaddressは10進数としてリトルエンディアンでu8;3に変換する。
		// code.baseが16の場合はaddressは16進数の場合はリトルエンディアンでu8;3に変換する。
		match param.base{
			10 => {
				match address.parse::<u32>(){
					Err(_) => None,
					Ok(x) => {
						let mut s:[u8;4] = x.to_le_bytes();
						s[3] = param.code;
						Some(s)
					},
				}
			},
			16 => {
        match u32::from_str_radix(&address, 16){
          Ok(v) => {
            let mut s:[u8;4] = v.to_le_bytes();
            s[3]=param.code;
            Some(s)
          },
          Err(_) => None,
        }
      },
			_ => None,
		}
	}

	pub fn get_device_info(device:&str)->Option<&DeviceInfo>{
		// device に含まれるデバイス名をDeviceListのDeciceCodeのnameから検索する。
		// デバイス名が存在しない場合はNoneを返す。
		DEVICE_LIST.iter().find(|&x| device.starts_with(x.name))
	}

	// デバイスの文字列からデバイスの情報を文字列で返す
	pub fn get_device_info_string(device:&str)->Option<String>{
		get_device_info(device).map(|x| format!("{}({})",x.description,x.class.to_string()))
	}
	


}
	// Qシリーズのデバイスコード定義





