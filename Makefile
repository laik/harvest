linux:
	CROSS_COMPILE=x86_64-linux-musl- cargo build --release --target x86_64-unknown-linux-musl

run:
	cargo build --release
	./target/release/harvest --namespace kube-system --docker-dir ${PWD}/tmp --api-server http://localhost:9999/ --host node1
	
docker:
	docker build -t yametech/harvest:latest .
	docker push yametech/harvest:latest