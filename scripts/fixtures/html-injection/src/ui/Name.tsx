export function Name(props: { name: string; el: HTMLElement }) {
  props.el.innerHTML = props.name;
  props.el["outerHTML"] = props.name;
  props.el.insertAdjacentHTML("beforeend", props.name);
  document.write(props.name);
  return <span dangerouslySetInnerHTML={{ __html: props.name }} />;
}
