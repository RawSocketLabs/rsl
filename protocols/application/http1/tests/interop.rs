//! Live interoperability: the blocking exchange against Python's `http.server` on loopback.
//!
//! Ignored by default because it needs `python3` and a free loopback port. Run it with
//! `cargo test -p http1 --test interop -- --ignored`.

mod interop {
    use std::{
        env, fs,
        net::TcpStream,
        process::{Child, Command, Stdio},
        thread,
        time::Duration,
    };

    use http1::{Limits, Request, blocking::exchange};

    const ADDRESS: &str = "127.0.0.1:44481";

    /// Kills the server when the test ends, however it ends.
    struct Server(Child);

    impl Drop for Server {
        fn drop(&mut self) {
            self.0.kill().ok();
            self.0.wait().ok();
        }
    }

    #[test]
    #[ignore = "needs python3 and a free loopback port"]
    fn exchange_fetches_a_file_from_pythons_http_server() {
        let directory = env::temp_dir().join(format!("http1-interop-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let content = "fetched over http1\n".repeat(2000); // several reads' worth
        fs::write(directory.join("file.txt"), &content).unwrap();
        let _server = Server(
            Command::new("python3")
                .args(["-I", "-m", "http.server", "--bind", "127.0.0.1", "44481"])
                .current_dir(&directory)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let connect = || {
            let stream = (0..50).find_map(|_| {
                thread::sleep(Duration::from_millis(100));
                TcpStream::connect(ADDRESS).ok()
            });
            let stream = stream.expect("python http.server did not start");
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            stream
        };

        let mut stream = connect();
        let request = Request::get(ADDRESS, "/file.txt").unwrap();
        let response = exchange(&mut stream, &request, Limits::default()).unwrap();
        assert_eq!(response.head.status, 200);
        assert_eq!(response.body, content.as_bytes());
        assert!(
            !response.keep_alive,
            "http.server answers HTTP/1.0 and closes"
        );

        let mut stream = connect();
        let missing = Request::get(ADDRESS, "/missing.txt").unwrap();
        let response = exchange(&mut stream, &missing, Limits::default()).unwrap();
        assert_eq!(response.head.status, 404);

        fs::remove_dir_all(&directory).ok();
    }
}
