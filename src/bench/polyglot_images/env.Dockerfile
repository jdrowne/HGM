FROM --platform={platform} {base_image}

COPY ./setup_env.sh /root/
RUN /bin/bash /root/setup_env.sh

WORKDIR /testbed/

ENV PATH=/opt/miniconda3/envs/testbed/bin:$PATH
RUN echo "source /opt/miniconda3/etc/profile.d/conda.sh && conda activate testbed" > /root/.bashrc
