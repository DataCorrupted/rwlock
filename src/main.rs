extern crate rwlock;
use std::thread;
use std::sync::Arc;
use rwlock::*;

fn main(){
	let important = 42;
	let lock = Arc::new(RwLock::new(important, Preference::Reader, Order::Lifo));
	let mut children: Vec<_> = Vec::new();
	for i in 0..100 {
		let cnt = i;
		let lock_ref = lock.clone();
		children.push(thread::spawn(move || {
			if cnt % 3 != 0{
				//print!("Trying to write...\n");
				//let mut lock_i = lock_ref.write(&cnt).unwrap();
				let mut lock_i = lock_ref.write().unwrap();
				*lock_i = i;
				print!("Iter: {}. Write Successful, important value become: {}\n", cnt, *lock_i);
			} else {
				//print!("Trying to read...\n");
				//let lock_i = lock_ref.read(&cnt).unwrap();
				let lock_i = lock_ref.read().unwrap();
				print!("Iter: {}. Read Successful, important value is: {}\n", cnt, *lock_i);
			}
		}));	
	}
	for child in children {
		let _ = child.join();
	}
}