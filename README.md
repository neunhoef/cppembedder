# cppembedder - vector embeddings for C++ projects

This project uses the language server `clangd` to cut the source code of a
C++ project into chunks and then computes a vector embedding using fastembed
withconfigurable text embedding locally. Finally, the created data about
the project is inserted into ArangoDB for similarity search.
