all: build build-front rollover

build:
	docker buildx build --platform linux/amd64 . --tag registry.danya02.ru/danya02/rudn-yamadharma-course/builder:latest --builder local --push

build-front:
	docker buildx build --platform linux/amd64,linux/arm64 -f Dockerfile.frontend . --tag registry.danya02.ru/danya02/rudn-yamadharma-course/front:latest --builder local --push

redeploy:
	kubectl delete -f deploy.yaml ; exit 0
	sleep 2
	kubectl apply -f deploy.yaml

rollover:
	kubectl -n rudn-yamadharma rollout restart deployment/pandoc-builder
	kubectl -n rudn-yamadharma rollout restart deployment/front

deploy:
	kubectl apply -f deploy.yaml

delete:
	kubectl delete -f deploy.yaml ; exit 0

log:
	kubectl logs -n ao3 -f deployment/find-new -f


initialize_builder:
	docker buildx create --bootstrap --name=local --driver=docker-container --platform=linux/arm64,linux/amd64

delete_builder:
	docker buildx rm local
