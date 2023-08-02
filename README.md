# Version

translations: en_US | [ru-RU](./README.ru.md)

_How It Works:_

1. Follow the Conventional Commits Specification in your repository.
2. When you're ready to release, run `version`.

`version` will then do the following:

1. Retrieve the current version of your repository by looking at the last git tag.
2. bump the version in files (composer.json, package.json, Chart.yaml) based on your commits.
3. Generates a changelog based on your commits.
4. Creates a new commit including your files and updated CHANGELOG.
5. Creates a new tag with the new version number.

_Why:_ In my daily job routine, I use GitLab Ci and deploy projects whenever a new tag is created.
Because of this, I need a tool that will create a tag, write a changelog, and bump the package version in all files in the project, so I will not do it by myself.

## CLI Usage

```console
user@pc:~$ version
```
