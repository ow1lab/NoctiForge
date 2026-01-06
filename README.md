# NoctiForge

**NoctiForge** is a self-hosted function runtime that lets you focus on writing code—not managing infrastructure.

Just write a handler. NoctiForge handles the rest.

---

# Usage
NoctiForge is build with the developer in mind. so to make it easy to develop i have provided a cli tool that can used to quickly develop it.
```sh
./cli push {folder}             # build & push a single function
./cli push all                  # build & push all functions in the project
./cli invoke {name} ({body})    # run a function locally
./cli list                      # list all functions in the registry
```

## Development
### Prerequisites
Ensure you have the following installed:
- Cargo (Version 1.86.0)

| If you are using Nix, the dependencies will be installed automatically. 

Initialize the required folders and files:
```sh
./scripts/setup.sh
```
 
## Architecture
![noctiforge infra](./assert/InfraDiagram.svg)

## Contribute

Want to help shape NoctiForge? We welcome contributions! Whether it's adding a feature, improving docs, or creating an SDK, you're invited.

---

## License

Apache-2.0
