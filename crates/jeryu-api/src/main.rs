use jeryu_api::Router;

fn main() {
    let mut router = Router::default();
    let response = router.get("/api/phase10/ready");
    println!("{} {}", response.status, response.body);
}
