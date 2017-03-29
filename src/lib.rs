use std::cell::UnsafeCell;
use std::ops::{Deref, DerefMut};
use std::sync::{Mutex, Condvar};

struct State{
	actv_reader: i32,
	actv_writer: i32,
	wtng_reader: i32,
	wtng_writer: i32,
}
// Provides a reader-writer lock to protect data of type `T`
pub struct RwLock<T> {
	data: UnsafeCell<T>,
	pref: Preference,
	order: Order,
	state: Mutex<State>,
	reader: Condvar,
	writer: UnsafeCell<Vec<Condvar>>,
}

#[derive(PartialEq)]
pub enum Preference {
    // Readers-preferred
    // * Readers must wait when a writer is active.
    // * Writers must wait when a reader is active or waiting, or a writer is active.
    Reader,
    // Writers-preferred: 
    // * Readers must wait when a writer is active or waiting.
    // * Writer must wait when a reader or writer is active.
    Writer,
}

// In which order to schedule threads
pub enum Order {
    // First in first out
    Fifo,
    // Last in first out
    Lifo,
}
impl<T> RwLock<T> {
	// Constructs a new `RwLock`
	//
	// data: the shared object to be protected by this lock
	// pref: which preference
	// order: in which order to wake up the threads wtng on this lock
	pub fn new(data: T, pref: Preference, order: Order) -> RwLock<T> {
		RwLock{ 
			data: UnsafeCell::new(data), 
			pref: pref, order: order, 
			state: Mutex::new(State{ 
				actv_reader: 0, actv_writer: 0,
				wtng_reader: 0, wtng_writer: 0
			}),
			reader: Condvar::new(),
			writer: UnsafeCell::new(Vec::new()),
		}
	}

	// Requests a read lock, waits when necessary, and wakes up as soon as the lock becomes available.
	// 
	// Always returns Ok(_).
	// (We declare this return type to be `Result` to be compatible with `std::sync::RwLock`)
	pub fn read(&self) -> Result<RwLockReadGuard<T>, ()> {
		let mut state = self.state.lock().unwrap();
		state.wtng_reader += 1;
		match self.pref {
			Preference::Reader 	=> {
				while state.actv_writer > 0 {
					state = self.reader.wait(state).unwrap();
				}
			},
			Preference::Writer 	=> {
				while state.actv_writer + state.wtng_writer > 0{
					state = self.reader.wait(state).unwrap();
				}				
			},
		}
		state.actv_reader += 1;
		state.wtng_reader -= 1;
		Ok(RwLockReadGuard{ lock: &self })	
	}

	// Requests a write lock, and waits when necessary.
	// When the lock becomes available,
	// * if `order == Order::Fifo`, wakes up the first thread
	// * if `order == Order::Lifo`, wakes up the last thread
	// 
	// Always returns Ok(_).
	pub fn write(&self) -> Result<RwLockWriteGuard<T>, ()> {
		let mut state = self.state.lock().unwrap();
		state.wtng_writer += 1;
		let vec = unsafe{ &mut *self.writer.get() };
		let len = vec.len();
		vec.push(Condvar::new());
		match self.pref{
			Preference::Reader 	=> {
				while state.actv_writer + state.actv_reader + state.wtng_reader > 0{
					state = vec[len].wait(state).unwrap();
				}
			},
			Preference::Writer 	=> {
				while state.actv_writer + state.actv_reader > 0{
					state = vec[len].wait(state).unwrap();
				}
			},
		}
		state.actv_writer += 1;
		state.wtng_writer -= 1;
		Ok(RwLockWriteGuard{ lock: &self })
	}
}

// Declares that it is safe to send and reference `RwLock` between threads safely
unsafe impl<T: Send + Sync> Send for RwLock<T> {}
unsafe impl<T: Send + Sync> Sync for RwLock<T> {}

// A read guard for `RwLock`
pub struct RwLockReadGuard<'a, T: 'a> {
	lock: &'a RwLock<T>,
}
// A write guard for `RwLock`
pub struct RwLockWriteGuard<'a, T: 'a> {
	lock: &'a RwLock<T>,
}

// Releases the read lock
impl<'a, T> Drop for RwLockReadGuard<'a, T> {
	fn drop(&mut self){
		let mut state = self.lock.state.lock().unwrap();
		state.actv_reader -= 1;
		if state.wtng_writer > 0 {
			let vec = unsafe{ &mut *self.lock.writer.get() };
			match &self.lock.order{
				&Order::Fifo	=>{
					vec[0].notify_all();
					vec.remove(0);
				},
				&Order::Lifo	=>{
					let temp = vec.pop().unwrap();
					temp.notify_all();
				},
			}
		}
	}
}

// Releases the write lock
impl<'a, T> Drop for RwLockWriteGuard<'a, T> {
	fn drop(&mut self){
		let mut state = self.lock.state.lock().unwrap();
		state.actv_writer -= 1;
		match self.lock.pref {
			Preference::Reader 	=>{
				if state.wtng_reader > 0 {
					self.lock.reader.notify_all();
				} else if state.wtng_writer > 0 {
					let vec = unsafe{ &mut *self.lock.writer.get() };
					match self.lock.order{
						Order::Fifo	=>{
							vec[0].notify_all();
							vec.remove(0);
						},
						Order::Lifo	=>{
							let temp = vec.pop().unwrap();
							temp.notify_all();
						},
					}
				}
			},
			Preference::Writer 	=>{
				if state.wtng_writer > 0 {
					let vec = unsafe{ &mut *self.lock.writer.get() };
					match self.lock.order{
						Order::Fifo	=>{
							vec[0].notify_all();
							vec.remove(0);
						},
						Order::Lifo	=>{
							let temp = vec.pop().unwrap();
							temp.notify_all();
						},
					}
				} else if state.wtng_reader > 0 {
					self.lock.reader.notify_all();
				}
			},
		}
	}
}

// Provides access to the shared object
impl<'a, T> Deref for RwLockReadGuard<'a, T> {
	type Target = T;
	fn deref(&self) -> &T {
		unsafe{ & *self.lock.data.get() }
	}
}
// Provides access to the shared object
impl<'a, T> Deref for RwLockWriteGuard<'a, T> {
	type Target = T;
	fn deref(&self) -> &T {
		unsafe{ & *self.lock.data.get() }
	}	
}
// Provides access to the shared object
impl<'a, T> DerefMut for RwLockWriteGuard<'a, T> {
	fn deref_mut(&mut self) -> &mut T {
		unsafe{ &mut *self.lock.data.get() }
	}
}

/*
#[test]
fn test_lock(){
	let important = 12;
	let imp_lock = RwLock::new(important, Preference::Reader, Order::Fifo);
{	let mut lock1 = imp_lock.write().unwrap();
	*lock1 += 1;	}
	let lock2 = imp_lock.read().unwrap();
	let lock3 = imp_lock.read().unwrap();
	assert_eq!(13, *lock2);
	assert_eq!(13, *lock3);
}*/