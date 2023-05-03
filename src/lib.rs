pub mod melsec_device{
	pub mod qseries;
}

pub mod mc_protocol{

	use tokio::net::TcpStream;
	use tokio::io::AsyncWriteExt;
	use tokio::io::AsyncReadExt;
	use crate::melsec_device::qseries::melsec_qseries;

	pub struct Config {
		pub network_no:u8,
		pub pc_no:u8,
		pub request_unit_io_no:u16,
		pub request_unit_station_no:u8,
		pub monitor_timer:u16,
	}
	impl Config {

		const SUBHEADER:[u8;2] = [0x50,0x00];

		pub fn new(
			networkno:u8,
			pcno:u8,
			req_unit_io_no:u16,
			req_unit_station_no:u8,
			mon_tim:u16,
		) -> Self {
			Self {
				network_no:networkno,
				pc_no:pcno,
				request_unit_io_no:req_unit_io_no,
				request_unit_station_no:req_unit_station_no,
				monitor_timer:mon_tim,
			}
		}

		pub fn create_send_message(&self,command_msg:&[u8]) -> Vec<u8> {
			let request_data_length = command_msg.len() as u16 + 2;
			let mut message:Vec<u8> = Vec::new();
			message.extend_from_slice(Config::SUBHEADER.as_ref());
			message.push(self.network_no);
			message.push(self.pc_no);
			message.extend_from_slice(&self.request_unit_io_no.to_le_bytes());
			message.push(self.request_unit_station_no);
			message.extend_from_slice(&request_data_length.to_le_bytes());
			message.extend_from_slice(&self.monitor_timer.to_le_bytes());
			message.extend_from_slice(command_msg);
			message
		}

	}


	// MCプロトコルの構造体を定義
	pub struct QMCProtocol {
		ip:(u8,u8,u8,u8),
		port:u16,
		config:Config,
		stream:Option<TcpStream>
	}

	// MCProtocolのメソッドを定義
	impl QMCProtocol {
	

		// MCProtocolのコンストラクタ
		pub fn new(
			ip_address:(u8,u8,u8,u8),
			port_number:u16,
			conf:Config,
		) -> Self {
			Self {
				ip:ip_address,
				port:port_number,
				config:conf,
				stream:None,
			}
		}

		// 接続文字列を返すメソッド
		fn get_connection_string(&self) -> String {
			format!(
				"{}.{}.{}.{}:{}",
				self.ip.0,
				self.ip.1,
				self.ip.2,
				self.ip.3,
				self.port
			)
		}

		// TCP Stream ポート開放処理
		// 失敗したらエラーを返す
		pub async fn connect(&mut self) -> Result<(),std::io::Error> {
			// 接続文字列を取得
			let connection_string = self.get_connection_string();
			// 接続文字列をもとにTCPストリームを開く
			let stream = TcpStream::connect(connection_string).await?;
			self.stream = Some(stream);
			Ok(())
		}
	
		// 送受信処理
		async fn communication(&mut self,send_msg:&[u8],rcv_size:usize)->Result<Vec<u8>,String>{
			const INFO_SIZE:usize = 2 + 1 + 1 + 2 + 1 + 2 + 2;
			match self.stream.as_mut(){
				Some(s) => {
					match s.write_all(send_msg).await{
						Ok(_) => {},
						Err(e) => return Err(format!("送信エラー:{}",e)),
					}
				},
				None => return Err("ストリームが開かれていません".to_string()),
			}
			let mut buffer:Vec<u8> = vec![0;rcv_size+10];
			let stream = match self.stream.as_mut(){
				Some(s) => s,
				None => return Err("ストリームが開かれていません".to_string()),
			};
			let read_size = match stream.read(&mut buffer).await{
				Ok(s) => s,
				Err(e) => return Err(format!("受信エラー:{}",e)),
			};
			if read_size != rcv_size {
				// buffer.iter().take(read_size).for_each(|x| print!("{:02X} ",x));
				let error_code = format!("{:X}{:X}",buffer[10],buffer[9]);
				return Err(format!("受信データが不正です:受信サイズ{}/{}バイト(予定/実際) ErrorCode:{}",rcv_size,read_size,error_code));
			}

			let mut result:Vec<u8> = Vec::new();
			buffer.iter().take(read_size).skip(INFO_SIZE).for_each(|x| result.push(*x));
			Ok(result)
		}
	
		// ワード単位の一括読み出し(u16で返す)
		pub async fn block_read_word_raw(&mut self,device:&str,read_size:u16) -> Result<Vec<u16>,String> {

			if !(1..=960).contains(&read_size){
				return Err(format!("読み出しサイズが不正です:{}点",read_size));
			}

			const COMMAND:[u8;2] = [0x01,0x04];
			const SUB_COMMAND:[u8;2] = [0x00,0x00];
			
			// デバイス名をバイト列に変換
			let device_bytes = match melsec_qseries::to_device_code(device){
				Some(d) => d,
				None => return Err("デバイス名が不正です".to_string()),
			};

			let mut command_msg:Vec<u8> = Vec::new();
			command_msg.extend_from_slice(&COMMAND);
			command_msg.extend_from_slice(&SUB_COMMAND);
			command_msg.extend_from_slice(&device_bytes);
			command_msg.extend_from_slice(&read_size.to_le_bytes());

			let send_msg = self.config.create_send_message(&command_msg);
			let result = self.communication(&send_msg,11+2*read_size as usize).await?;

			// u8のベクタを2つずつまとめてu16に変換
			let mut result_u16:Vec<u16> = Vec::new();
			for i in 0..read_size as usize{
				let mut tmp:u16 = 0;
				tmp += result[2*i] as u16;
				tmp += (result[2*i+1] as u16) << 8;
				result_u16.push(tmp);
			}

			Ok(result_u16)
		}

		// ワード単位の一括読み出し
		pub async fn block_read_word(&mut self,device:&str,read_size:u16) -> Result<Vec<i16>,String> {
			// 生データを取得
			let result = self.block_read_word_raw(device, read_size).await?;
			// u16のベクタをi16に変換
			Ok(result.iter().take(read_size as usize).map(|x| *x as i16).collect())
		}

		// ビット単位の一括読み出し
		pub async fn block_read_bit(&mut self,device:&str,read_size:u16) -> Result<Vec<u8>,String> {

			const COMMAND:[u8;2] = [0x01,0x04];
			const SUB_COMMAND:[u8;2] = [0x01,0x00];
			
			// デバイス名をバイト列に変換
			let device_bytes = match melsec_qseries::to_device_code(device){
				Some(d) => d,
				None => return Err("デバイス名が不正です".to_string()),
			};

			let mut command_msg:Vec<u8> = Vec::new();
			command_msg.extend_from_slice(&COMMAND);
			command_msg.extend_from_slice(&SUB_COMMAND);
			command_msg.extend_from_slice(&device_bytes);
			command_msg.extend_from_slice(&read_size.to_le_bytes());

			let send_message = self.config.create_send_message(&command_msg);
			// サブヘッダ(2)+ネットワーク番号(1)+PC番号(1)+要求先ユニットI/O番号(2)+要求先ユニット局番号(1)+応答データ長(2)+終了コード(2)+応答データ(2*read_size)
			const INFO_SIZE:usize = 2 + 1 + 1 + 2 + 1 + 2 + 2;
			// read_size を2で割った値が奇数の場合は1バイト余分に受信する
			let recv_data_size = INFO_SIZE + if read_size %2 == 0 {read_size as usize / 2} else {read_size as usize / 2 + 1};

			let raw_data =  self.communication(&send_message,recv_data_size).await?;
			// Vec<u8>をVec<u4>に変換
			let mut data:Vec<u8> = Vec::new();
			for i in 0..read_size {
				let byte = raw_data[i as usize/2];
				let bit = if i % 2 == 0 {4} else {0};
				data.push((byte >> bit) & 0x01);
			}

			Ok(data)

		}
		
		// ワード単位の一括書き込み
		pub async fn block_write_word(&mut self,device:&str,write_data:&[i16]) -> Result<(),String> {

			const COMMAND:[u8;2] = [0x01,0x14];
			const SUB_COMMAND:[u8;2] = [0x00,0x00];
			
			// デバイス名をバイト列に変換
			let device_bytes = match melsec_qseries::to_device_code(device){
				Some(d) => d,
				None => return Err("デバイス名が不正です".to_string()),
			};

			let write_size = write_data.len() as u16;
			if !(1..=960).contains(&write_size){
				return Err(format!("書き込みサイズが不正です:{}点",write_size));
			}

			let mut command_msg:Vec<u8> = Vec::new();
			command_msg.extend_from_slice(&COMMAND);
			command_msg.extend_from_slice(&SUB_COMMAND);
			command_msg.extend_from_slice(&device_bytes);
			command_msg.extend_from_slice(&write_size.to_le_bytes());
			command_msg.extend_from_slice(&write_data.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>());

			let send_message = self.config.create_send_message(&command_msg);
			// サブヘッダ(2)+ネットワーク番号(1)+PC番号(1)+要求先ユニットI/O番号(2)+要求先ユニット局番号(1)+応答データ長(2)+終了コード(2)
			const INFO_SIZE:usize = 2 + 1 + 1 + 2 + 1 + 2 + 2;
			let recv_data_size = INFO_SIZE;

			let raw_data = self.communication(&send_message,recv_data_size).await?;

			if !raw_data.is_empty() {
				return Err(format!("書き込みエラー:エラーコード{:02X}{:02X}",raw_data[0],raw_data[1]));
			}
			Ok(())
		}

		// ビット単位の一括書き込み
		pub async fn block_write_bit(&mut self,device:&str,write_data:&[u8]) -> Result<(),String> {

			const COMMAND:[u8;2] = [0x01,0x14];
			const SUB_COMMAND:[u8;2] = [0x01,0x00];
			
			// デバイス名をバイト列に変換
			let device_bytes = match melsec_qseries::to_device_code(device){
				Some(d) => d,
				None => return Err("デバイス名が不正です".to_string()),
			};

			let write_size = write_data.len() as u16;
			if !(1..=7168).contains(&write_size){
				return Err(format!("書き込みサイズが不正です:{}点",write_size));
			}

			// u8の配列を
			let bin_str = write_data.iter().map(|x| x.to_string()).collect::<Vec<String>>().join("");
			println!("{:?}",&bin_str);
			// bin_strの長さが2の倍数になるように末尾に0を追加
			let bin_str = if &bin_str.len() % 2 == 0 {bin_str} else {bin_str.to_string() + &"0".repeat(2 - &bin_str.len() % 2)};
			// 2文字ずつに分割
			let bin_str_arr = bin_str.chars().collect::<Vec<char>>().chunks(2).map(|x| x.iter().collect::<String>()).collect::<Vec<String>>();
			println!("{:?}",&bin_str_arr);
			
			// 16進数文字列が格納されているbin_str_arrをu8の配列に変換
			let data = bin_str_arr.iter().map(|x| u8::from_str_radix(x,16).unwrap()).collect::<Vec<u8>>();

			let mut command_msg:Vec<u8> = Vec::new();
			command_msg.extend_from_slice(&COMMAND);
			command_msg.extend_from_slice(&SUB_COMMAND);
			command_msg.extend_from_slice(&device_bytes);
			command_msg.extend_from_slice(&write_size.to_le_bytes());
			command_msg.extend_from_slice(&data);

			let send_message = self.config.create_send_message(&command_msg);

			println!("{:02X?}",&send_message);
			// サブヘッダ(2)+ネットワーク番号(1)+PC番号(1)+要求先ユニットI/O番号(2)+要求先ユニット局番号(1)+応答データ長(2)+終了コード(2)
			const INFO_SIZE:usize = 2 + 1 + 1 + 2 + 1 + 2 + 2;
			let recv_data_size = INFO_SIZE;

			let raw_data = self.communication(&send_message,recv_data_size).await?;

			if !raw_data.is_empty() {
				return Err(format!("書き込みエラー:エラーコード{:02X}{:02X}",raw_data[0],raw_data[1]));
			}
			Ok(())
		}


	}


}


